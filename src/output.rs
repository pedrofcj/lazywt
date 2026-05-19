use owo_colors::OwoColorize;
use owo_colors::Stream;

// Unicode symbol constants matching the shell scripts exactly
pub const CHECK: &str = "\u{2713}"; // checkmark
pub const CROSS: &str = "\u{2717}"; // X mark
pub const STAR: &str = "\u{2605}";  // filled star
pub const ARROW: &str = "\u{25B8}"; // right-pointing triangle

// Upstream relationship symbols
pub const UPSTREAM_AHEAD: &str = "\u{21E1}";   // ⇡
pub const UPSTREAM_BEHIND: &str = "\u{21E3}";  // ⇣
pub const UPSTREAM_DIVERGE: &str = "\u{21C5}"; // ⇅
pub const UPSTREAM_SYNC: &str = "|";            // |
pub const UPSTREAM_NONE: &str = "\u{2014}";    // —

// Main branch relationship symbols
pub const MAIN_IS_DEFAULT: &str = "^";
pub const MAIN_AHEAD: &str = "\u{2191}";       // ↑
pub const MAIN_BEHIND: &str = "\u{2193}";      // ↓
pub const MAIN_DIVERGE: &str = "\u{2195}";     // ↕
pub const MAIN_INTEGRATED: &str = "\u{2282}";  // ⊂
pub const MAIN_CONFLICT: &str = "\u{2717}";    // ✗
pub const MAIN_SAME_COMMIT: &str = "_";
pub const MAIN_ORPHAN: &str = "\u{2205}";      // ∅

// Active operation symbols
pub const OP_REBASE: &str = "\u{2934}";        // ⤴
pub const OP_MERGE: &str = "\u{2935}";         // ⤵

// Worktree state symbols
pub const LOCKED: &str = "\u{229E}";           // ⊞
pub const PRUNABLE: &str = "\u{229F}";         // ⊟

/// Status variants for progress display
pub enum Status {
    Success,
    Warning,
    Error,
    Info,
}

/// Print a success message: "checkmark msg" in green to stdout
pub fn success(msg: &str) {
    println!(
        "{} {}",
        CHECK.if_supports_color(Stream::Stdout, |s| s.green()),
        msg.if_supports_color(Stream::Stdout, |s| s.green())
    );
}

/// Print an error message: "X msg" in red to stderr
pub fn error(msg: &str) {
    eprintln!(
        "{} {}",
        CROSS.if_supports_color(Stream::Stderr, |s| s.red()),
        msg.if_supports_color(Stream::Stderr, |s| s.red())
    );
}

/// Print a warning message in yellow to stderr
pub fn warning(msg: &str) {
    eprintln!(
        "{}",
        msg.if_supports_color(Stream::Stderr, |s| s.yellow())
    );
}

/// Print an info message in cyan to stdout
pub fn info(msg: &str) {
    println!(
        "{}",
        msg.if_supports_color(Stream::Stdout, |s| s.cyan())
    );
}

/// Print a section header: "=== msg ===" in blue to stdout
pub fn header(msg: &str) {
    println!(
        "{}",
        format!("=== {} ===", msg)
            .if_supports_color(Stream::Stdout, |s| s.blue())
    );
}

/// Start a progress message: "msg..." to stderr (no newline at end)
pub fn progress_start(msg: &str) {
    eprint!("{}...", msg);
}

/// Complete a progress display with symbol and message on a new line to stderr
pub fn progress_complete(msg: &str, status: Status) {
    match status {
        Status::Success => {
            eprintln!(
                "\n{} {}",
                CHECK.if_supports_color(Stream::Stderr, |s| s.green()),
                msg.if_supports_color(Stream::Stderr, |s| s.green())
            );
        }
        Status::Warning => {
            eprintln!(
                "\n{} {}",
                CROSS.if_supports_color(Stream::Stderr, |s| s.yellow()),
                msg.if_supports_color(Stream::Stderr, |s| s.yellow())
            );
        }
        Status::Error => {
            eprintln!(
                "\n{} {}",
                CROSS.if_supports_color(Stream::Stderr, |s| s.red()),
                msg.if_supports_color(Stream::Stderr, |s| s.red())
            );
        }
        Status::Info => {
            eprintln!(
                "\n{} {}",
                ARROW.if_supports_color(Stream::Stderr, |s| s.cyan()),
                msg.if_supports_color(Stream::Stderr, |s| s.cyan())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use owo_colors::set_override;

    #[test]
    fn check_symbol_is_checkmark() {
        assert_eq!(CHECK, "\u{2713}");
        assert_eq!(CHECK, "\u{2713}");
    }

    #[test]
    fn cross_symbol_is_x_mark() {
        assert_eq!(CROSS, "\u{2717}");
    }

    #[test]
    fn star_symbol_is_filled_star() {
        assert_eq!(STAR, "\u{2605}");
    }

    #[test]
    fn arrow_symbol_is_right_triangle() {
        assert_eq!(ARROW, "\u{25B8}");
    }

    #[test]
    fn upstream_ahead_symbol() { assert_eq!(UPSTREAM_AHEAD, "\u{21E1}"); }
    #[test]
    fn upstream_behind_symbol() { assert_eq!(UPSTREAM_BEHIND, "\u{21E3}"); }
    #[test]
    fn upstream_diverge_symbol() { assert_eq!(UPSTREAM_DIVERGE, "\u{21C5}"); }
    #[test]
    fn upstream_sync_symbol() { assert_eq!(UPSTREAM_SYNC, "|"); }
    #[test]
    fn upstream_none_symbol() { assert_eq!(UPSTREAM_NONE, "\u{2014}"); }
    #[test]
    fn main_is_default_symbol() { assert_eq!(MAIN_IS_DEFAULT, "^"); }
    #[test]
    fn main_ahead_symbol() { assert_eq!(MAIN_AHEAD, "\u{2191}"); }
    #[test]
    fn main_behind_symbol() { assert_eq!(MAIN_BEHIND, "\u{2193}"); }
    #[test]
    fn main_diverge_symbol() { assert_eq!(MAIN_DIVERGE, "\u{2195}"); }
    #[test]
    fn main_integrated_symbol() { assert_eq!(MAIN_INTEGRATED, "\u{2282}"); }
    #[test]
    fn main_conflict_symbol() { assert_eq!(MAIN_CONFLICT, "\u{2717}"); }
    #[test]
    fn main_same_commit_symbol() { assert_eq!(MAIN_SAME_COMMIT, "_"); }
    #[test]
    fn main_orphan_symbol() { assert_eq!(MAIN_ORPHAN, "\u{2205}"); }
    #[test]
    fn op_rebase_symbol() { assert_eq!(OP_REBASE, "\u{2934}"); }
    #[test]
    fn op_merge_symbol() { assert_eq!(OP_MERGE, "\u{2935}"); }
    #[test]
    fn locked_symbol() { assert_eq!(LOCKED, "\u{229E}"); }
    #[test]
    fn prunable_symbol() { assert_eq!(PRUNABLE, "\u{229F}"); }

    // Function body coverage tests -- call each output function to ensure
    // the println!/eprintln! bodies are exercised. Disable color override
    // to avoid ANSI escape issues in test output.

    #[test]
    fn success_function_runs() {
        owo_colors::set_override(false);
        success("test success message");
    }

    #[test]
    fn error_function_runs() {
        owo_colors::set_override(false);
        error("test error message");
    }

    #[test]
    fn warning_function_runs() {
        owo_colors::set_override(false);
        warning("test warning message");
    }

    #[test]
    fn info_function_runs() {
        owo_colors::set_override(false);
        info("test info message");
    }

    #[test]
    fn header_function_runs() {
        owo_colors::set_override(false);
        header("test header");
    }

    #[test]
    fn progress_start_function_runs() {
        owo_colors::set_override(false);
        progress_start("loading");
    }

    #[test]
    fn progress_complete_success_runs() {
        owo_colors::set_override(false);
        progress_complete("done", Status::Success);
    }

    #[test]
    fn progress_complete_warning_runs() {
        owo_colors::set_override(false);
        progress_complete("warn", Status::Warning);
    }

    #[test]
    fn progress_complete_error_runs() {
        owo_colors::set_override(false);
        progress_complete("err", Status::Error);
    }

    #[test]
    fn progress_complete_info_runs() {
        owo_colors::set_override(false);
        progress_complete("note", Status::Info);
    }
}
