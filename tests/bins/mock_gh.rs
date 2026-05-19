/// Mock `gh` CLI binary for PR command tests.
///
/// Reads behavior from environment variables:
/// - MOCK_GH_BRANCH: branch name to return from `gh pr view`
/// - MOCK_GH_PR_OWNER: PR author login to return from `gh pr view`
/// - MOCK_GH_REPO_OWNER: repo owner login to return from `gh repo view --json owner`
/// - MOCK_GH_REPO_NAME: repo name to return from `gh repo view --json name`
/// - MOCK_GH_EXIT_CODE: exit code to return (default: 0)
///
/// When MOCK_GH_EXIT_CODE is non-zero, the mock exits immediately with that code
/// (simulating gh CLI failure).
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args_str = args.join(" ");

    // Check for forced failure
    if let Ok(code) = env::var("MOCK_GH_EXIT_CODE") {
        if let Ok(code) = code.parse::<i32>() {
            if code != 0 {
                process::exit(code);
            }
        }
    }

    // Handle `gh pr view N --json ...`
    if args_str.contains("pr") && args_str.contains("view") {
        // MOCK_GH_EMPTY_OUTPUT: when set to "1", output only whitespace
        // (simulates gh returning no useful data, causing lines.next() -> None)
        if env::var("MOCK_GH_EMPTY_OUTPUT").unwrap_or_default() == "1" {
            println!();
            process::exit(0);
        }
        let branch = env::var("MOCK_GH_BRANCH").unwrap_or_default();
        let pr_owner = env::var("MOCK_GH_PR_OWNER").unwrap_or_default();
        println!("{}", branch);
        println!("{}", pr_owner);
        process::exit(0);
    }

    // Handle `gh repo view --json owner -q .owner.login`
    if args_str.contains("repo") && args_str.contains("view") && args_str.contains(".owner.login")
    {
        let repo_owner = env::var("MOCK_GH_REPO_OWNER").unwrap_or_default();
        println!("{}", repo_owner);
        process::exit(0);
    }

    // Handle `gh repo view --json name -q .name`
    if args_str.contains("repo") && args_str.contains("view") && args_str.contains(".name") {
        let repo_name = env::var("MOCK_GH_REPO_NAME").unwrap_or_default();
        println!("{}", repo_name);
        process::exit(0);
    }

    // Unknown command
    eprintln!("mock_gh: unhandled command: {}", args_str);
    process::exit(1);
}
