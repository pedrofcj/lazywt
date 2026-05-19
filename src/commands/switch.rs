use std::path::Path;

use anyhow::Result;

#[cfg(not(coverage))]
use std::io::IsTerminal;
#[cfg(not(coverage))]
use dialoguer::Select;

use crate::config::Config;
use crate::git::worktree::list_worktrees;
use crate::layout::ProjectContext;
use crate::navigate;
use crate::output;

pub fn run(ctx: &ProjectContext, config: &Config, name: Option<&str>) -> Result<()> {
    match name {
        Some("-") => switch_previous(ctx),
        Some(name) => switch_direct(ctx, name),
        None => switch_interactive(ctx, config),
    }
}

/// Switch to the previous worktree via LAZYWT_PREV environment variable.
fn switch_previous(_ctx: &ProjectContext) -> Result<()> {
    let prev = std::env::var("LAZYWT_PREV").map_err(|_| {
        anyhow::anyhow!(
            "No previous worktree \u{2014} LAZYWT_PREV is not set. \
             Use 'eval \"$(lazywt init)\"' in your shell profile."
        )
    })?;
    let prev_path = std::path::PathBuf::from(&prev);
    if !prev_path.exists() {
        anyhow::bail!("Previous worktree path no longer exists: {}", prev);
    }
    navigate::request_cd(&prev_path);
    Ok(())
}

/// Switch directly to a worktree by name with exact, substring, and fuzzy matching.
fn switch_direct(ctx: &ProjectContext, name: &str) -> Result<()> {
    let worktrees = list_worktrees(&ctx.project_dir)?;
    let non_bare: Vec<_> = worktrees.iter().filter(|w| !w.is_bare).collect();

    // Build directory-name -> path pairs
    let names: Vec<(&str, &Path)> = non_bare
        .iter()
        .map(|wt| {
            let dir_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            (dir_name, wt.path.as_path())
        })
        .collect();

    // Exact match first
    if let Some((_, path)) = names.iter().find(|(n, _)| *n == name) {
        navigate::request_cd(path);
        return Ok(());
    }

    // Substring match -- ONLY check if worktree name contains the input.
    // Do NOT check `name.contains(*n)` (reverse direction). The reverse condition
    // means typing a long name would match short worktree names, producing noisy
    // false positives.
    let substring_matches: Vec<_> = names.iter().filter(|(n, _)| n.contains(name)).collect();

    if substring_matches.len() == 1 {
        return prompt_fuzzy_match(name, substring_matches[0].0, substring_matches[0].1);
    }

    // Levenshtein distance <= 2 with min-length guard.
    // Only apply Levenshtein to names where BOTH input and candidate have length >= 3.
    let mut fuzzy_matches: Vec<(&str, &Path, usize)> = if name.len() >= 3 {
        names
            .iter()
            .filter(|(n, _)| n.len() >= 3)
            .map(|(n, p)| (*n, *p, levenshtein(name, n)))
            .filter(|(_, _, dist)| *dist <= 2 && *dist > 0)
            .collect()
    } else {
        Vec::new()
    };
    fuzzy_matches.sort_by_key(|(_, _, d)| *d);

    if fuzzy_matches.is_empty() && substring_matches.is_empty() {
        anyhow::bail!("No worktree found matching '{}'", name);
    }

    // Combine substring and fuzzy, deduplicate
    let all_candidates: Vec<(&str, &Path)> = if !substring_matches.is_empty() {
        substring_matches.into_iter().map(|(n, p)| (*n, *p)).collect()
    } else {
        fuzzy_matches.iter().map(|(n, p, _)| (*n, *p)).collect()
    };

    if all_candidates.len() == 1 {
        return prompt_fuzzy_match(name, all_candidates[0].0, all_candidates[0].1);
    }

    // Multiple candidates: print suggestions and exit with error code
    output::info(&format!("No exact match for '{}'. Did you mean:", name));
    for (candidate_name, _) in &all_candidates {
        output::info(&format!("  - {}", candidate_name));
    }
    anyhow::bail!(
        "Ambiguous match for '{}' -- {} candidates found",
        name,
        all_candidates.len()
    )
}

/// Prompt the user to confirm a fuzzy match using dialoguer::Confirm.
#[cfg(not(coverage))]
fn prompt_fuzzy_match(input: &str, candidate: &str, path: &Path) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "No exact match for '{}' (closest: '{}')",
            input,
            candidate
        );
    }
    let confirmed = dialoguer::Confirm::new()
        .with_prompt(format!("Did you mean: {}?", candidate))
        .default(false)
        .interact()?;
    if confirmed {
        navigate::request_cd(path);
    }
    Ok(())
}

/// Stub for coverage builds: no dialoguer dependency, just bail with descriptive message.
#[cfg(coverage)]
fn prompt_fuzzy_match(input: &str, candidate: &str, _path: &Path) -> Result<()> {
    anyhow::bail!(
        "No exact match for '{}' (closest: '{}')",
        input,
        candidate
    );
}

/// Interactive picker mode (original behavior, no args).
#[cfg(not(coverage))]
fn switch_interactive(ctx: &ProjectContext, _config: &Config) -> Result<()> {
    // Non-TTY guard (piped stdin, CI, etc.)
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("Interactive selection requires a terminal");
    }

    let worktrees = list_worktrees(&ctx.project_dir)?;
    let non_bare: Vec<_> = worktrees.iter().filter(|w| !w.is_bare).collect();

    if non_bare.is_empty() {
        output::warning("No worktrees found");
        return Ok(());
    }

    // Build display items: "name -> branch" or just "name" when they match
    let items: Vec<String> = non_bare
        .iter()
        .map(|wt| {
            let name = wt
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let branch = wt.branch.as_deref().unwrap_or("(detached)");
            if name == branch {
                name.to_string()
            } else {
                format!("{} -> {}", name, branch)
            }
        })
        .collect();

    // Detect current worktree for default selection (same logic as list.rs)
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| dunce::canonicalize(&p).ok());
    let default_idx = cwd.as_ref().and_then(|cwd| {
        non_bare.iter().position(|wt| {
            dunce::canonicalize(&wt.path)
                .map(|c| cwd.starts_with(&c))
                .unwrap_or(false)
        })
    });

    let mut select = Select::new()
        .with_prompt("Switch to worktree")
        .items(&items);
    if let Some(idx) = default_idx {
        select = select.default(idx);
    }

    match select.interact_opt()? {
        Some(idx) => {
            navigate::request_cd(&non_bare[idx].path);
        }
        None => {
            // User pressed Esc -- silent cancel
        }
    }

    Ok(())
}

/// Stub for coverage builds: no dialoguer/terminal dependency.
#[cfg(coverage)]
fn switch_interactive(_ctx: &ProjectContext, _config: &Config) -> Result<()> {
    anyhow::bail!("Interactive selection requires a terminal");
}

/// Compute the Levenshtein edit distance between two strings.
/// Implemented inline to avoid new dependencies (D-71).
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (m, n) = (a_chars.len(), b_chars.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Find worktree matches for a given input name.
/// Returns exact matches, substring matches, and fuzzy (Levenshtein) matches.
/// Used by switch_direct internally but exposed for testing.
#[cfg(test)]
#[derive(Debug, PartialEq)]
enum MatchResult {
    Exact(String),
    Substring(Vec<String>),
    Fuzzy(Vec<String>),
    None,
}

#[cfg(test)]
fn find_worktree_matches(input: &str, worktree_names: &[&str]) -> MatchResult {
    // Exact match
    if worktree_names.contains(&input) {
        return MatchResult::Exact(input.to_string());
    }

    // Substring match -- only check worktree_name.contains(input)
    let substring: Vec<String> = worktree_names
        .iter()
        .filter(|n| n.contains(input))
        .map(|n| n.to_string())
        .collect();

    if !substring.is_empty() {
        return MatchResult::Substring(substring);
    }

    // Levenshtein with min-length guard
    if input.len() >= 3 {
        let mut fuzzy: Vec<(String, usize)> = worktree_names
            .iter()
            .filter(|n| n.len() >= 3)
            .map(|n| (n.to_string(), levenshtein(input, n)))
            .filter(|(_, dist)| *dist <= 2 && *dist > 0)
            .collect();
        fuzzy.sort_by_key(|(_, d)| *d);

        if !fuzzy.is_empty() {
            return MatchResult::Fuzzy(fuzzy.into_iter().map(|(n, _)| n).collect());
        }
    }

    MatchResult::None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Levenshtein tests ---

    #[test]
    fn levenshtein_identical_strings() {
        assert_eq!(levenshtein("auth", "auth"), 0);
    }

    #[test]
    fn levenshtein_transposition() {
        assert_eq!(levenshtein("auth", "auht"), 2);
    }

    #[test]
    fn levenshtein_deletion() {
        assert_eq!(levenshtein("auth", "aut"), 1);
    }

    #[test]
    fn levenshtein_empty_first() {
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn levenshtein_empty_second() {
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn levenshtein_both_empty() {
        assert_eq!(levenshtein("", ""), 0);
    }

    // --- find_worktree_matches tests ---

    #[test]
    fn find_matches_exact_returns_single() {
        let names = vec!["auth", "main", "dev"];
        let result = find_worktree_matches("auth", &names);
        assert_eq!(result, MatchResult::Exact("auth".to_string()));
    }

    #[test]
    fn find_matches_substring_found() {
        let names = vec!["auth", "main", "dev"];
        let result = find_worktree_matches("au", &names);
        assert_eq!(
            result,
            MatchResult::Substring(vec!["auth".to_string()])
        );
    }

    #[test]
    fn find_matches_no_match_returns_none() {
        let names = vec!["auth", "main", "dev"];
        let result = find_worktree_matches("xyz", &names);
        assert_eq!(result, MatchResult::None);
    }

    #[test]
    fn find_matches_levenshtein_skipped_for_short_names() {
        // "a" is length 1, so Levenshtein should NOT fire even though
        // distance from "a" to "abc" is 2
        let names = vec!["abc", "def", "ghi"];
        let result = find_worktree_matches("a", &names);
        // "a" is a substring of "abc", so it matches via substring
        assert_eq!(
            result,
            MatchResult::Substring(vec!["abc".to_string()])
        );
    }

    #[test]
    fn find_matches_levenshtein_skipped_for_short_input_no_substring() {
        // "z" is length 1 with no substring match - should return None
        // even though levenshtein("z", "abc") == 3 (too far anyway)
        let names = vec!["abc", "def"];
        let result = find_worktree_matches("z", &names);
        assert_eq!(result, MatchResult::None);
    }

    #[test]
    fn find_matches_substring_is_one_directional() {
        // "authentication" contains "auth" but we're checking if worktree names
        // contain the input, not if the input contains the worktree name.
        // So searching "authentication" should NOT match "auth" worktree.
        let names = vec!["auth", "main"];
        let result = find_worktree_matches("authentication", &names);
        // "authentication" is NOT contained in "auth" (auth does not contain "authentication")
        // Levenshtein distance is too high (10)
        // So this should be None or Fuzzy depending on distance
        assert_eq!(result, MatchResult::None);
    }

    #[test]
    fn find_matches_fuzzy_with_valid_distance() {
        let names = vec!["auth", "main", "dev"];
        // "auht" -> levenshtein to "auth" = 2, within threshold
        let result = find_worktree_matches("auht", &names);
        assert_eq!(result, MatchResult::Fuzzy(vec!["auth".to_string()]));
    }

    #[test]
    fn find_matches_multiple_substring_matches() {
        let names = vec!["auth-login", "auth-signup", "main"];
        let result = find_worktree_matches("auth", &names);
        assert_eq!(
            result,
            MatchResult::Substring(vec![
                "auth-login".to_string(),
                "auth-signup".to_string(),
            ])
        );
    }

    #[test]
    fn find_matches_levenshtein_min_length_guard() {
        // Both input and candidate must be >= 3 for Levenshtein
        // "ab" (length 2) should NOT fuzzy match "abc" even though distance is 1
        let names = vec!["abc"];
        let result = find_worktree_matches("ab", &names);
        // "ab" IS a substring of "abc", so it matches via substring
        assert_eq!(
            result,
            MatchResult::Substring(vec!["abc".to_string()])
        );
    }

    #[test]
    fn find_matches_levenshtein_short_candidate_skipped() {
        // Candidate "ab" has length 2, should be skipped from Levenshtein
        let names = vec!["ab"];
        let result = find_worktree_matches("abc", &names);
        // "abc" does not contain as substring in "ab" - "ab" does not contain "abc"
        // Levenshtein skipped because "ab" is length 2
        assert_eq!(result, MatchResult::None);
    }

    #[test]
    fn find_matches_multiple_fuzzy_candidates() {
        // "autx" has Levenshtein distance 1 from "auth", "auto", and "autz"
        // All three should appear as fuzzy matches
        let names = vec!["auth", "auto", "autz", "main", "dev"];
        let result = find_worktree_matches("autx", &names);
        match result {
            MatchResult::Fuzzy(candidates) => {
                assert!(candidates.len() >= 2, "expected multiple fuzzy candidates, got: {:?}", candidates);
                assert!(candidates.contains(&"auth".to_string()), "should contain auth");
                assert!(candidates.contains(&"auto".to_string()), "should contain auto");
                assert!(candidates.contains(&"autz".to_string()), "should contain autz");
            }
            other => panic!("expected Fuzzy result, got: {:?}", other),
        }
    }

    #[test]
    fn find_matches_fuzzy_sorted_by_distance() {
        // "maix" has distance 1 from "main" (substitution) but distance 2 from "maiz" needs checking
        // levenshtein("devx", "dev") = 1 (insertion), levenshtein("devx", "dex") = 1
        // Better: "feax" -> "feat" distance 1, "feaz" distance 2
        // levenshtein("feax", "feat") = 1 (x->t), levenshtein("feax", "feaz") = 1 (x->z)
        // Use: "bugfix" vs "bugfiz" (d=1) and "bugfox" (d=2)
        let names = vec!["bugfox", "bugfiz", "main"];
        // levenshtein("bugfix", "bugfiz") = 1 (x->z), levenshtein("bugfix", "bugfox") = 1 (i->o)
        // Both distance 1 -- they should both appear. Let's use a clearer example.
        // "featx" -> "feat" not valid (length < 3 rule applies to candidate... no, "feat" is len 4)
        // levenshtein("featx", "feat") = 1, levenshtein("featx", "feats") = 1
        // Use names with clear distance difference:
        let names2 = vec!["auth", "main", "auxh"];
        // levenshtein("autx", "auth") = 1 (t->h? no: a-u-t-x vs a-u-t-h = 1),
        // levenshtein("autx", "auxh") = 2 (t->x, x->h)
        let result = find_worktree_matches("autx", &names2);
        match result {
            MatchResult::Fuzzy(candidates) => {
                // "auth" (distance 1) should come before "auxh" (distance 2)
                assert!(candidates.len() >= 2, "should have multiple candidates: {:?}", candidates);
                let auth_pos = candidates.iter().position(|c| c == "auth");
                let auxh_pos = candidates.iter().position(|c| c == "auxh");
                assert!(auth_pos.is_some(), "auth should be in candidates");
                assert!(auxh_pos.is_some(), "auxh should be in candidates");
                assert!(auth_pos.unwrap() < auxh_pos.unwrap(), "auth should come before auxh (closer distance)");
            }
            other => panic!("expected Fuzzy result, got: {:?}", other),
        }
    }
}
