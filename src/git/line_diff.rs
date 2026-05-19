use std::path::Path;
use anyhow::Result;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineDiff {
    pub insertions: u32,
    pub deletions: u32,
}

fn parse_numstat(output: &str) -> LineDiff {
    let mut diff = LineDiff::default();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            if let (Ok(ins), Ok(del)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                diff.insertions += ins;
                diff.deletions += del;
            }
        }
    }
    diff
}

pub fn line_diff_head(path: &Path) -> Result<LineDiff> {
    match super::git(path, &["diff", "--numstat", "HEAD"]) {
        Ok(output) => Ok(parse_numstat(&output)),
        Err(_) => Ok(LineDiff::default()),
    }
}

pub fn line_diff_refs(dir: &Path, range: &str) -> Result<LineDiff> {
    match super::git(dir, &["diff", "--numstat", range]) {
        Ok(output) => Ok(parse_numstat(&output)),
        Err(_) => Ok(LineDiff::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numstat_empty() {
        let diff = parse_numstat("");
        assert_eq!(diff, LineDiff::default());
    }

    #[test]
    fn parse_numstat_single_file() {
        let diff = parse_numstat("10\t3\tsrc/main.rs\n");
        assert_eq!(diff.insertions, 10);
        assert_eq!(diff.deletions, 3);
    }

    #[test]
    fn parse_numstat_multiple_files() {
        let output = "10\t3\tsrc/main.rs\n5\t0\tsrc/lib.rs\n0\t7\tsrc/old.rs\n";
        let diff = parse_numstat(output);
        assert_eq!(diff.insertions, 15);
        assert_eq!(diff.deletions, 10);
    }

    #[test]
    fn parse_numstat_binary_file_skipped() {
        let output = "-\t-\timage.png\n5\t2\tsrc/main.rs\n";
        let diff = parse_numstat(output);
        assert_eq!(diff.insertions, 5);
        assert_eq!(diff.deletions, 2);
    }

    #[test]
    fn parse_numstat_rename() {
        let output = "0\t0\t{old.rs => new.rs}\n";
        let diff = parse_numstat(output);
        assert_eq!(diff.insertions, 0);
        assert_eq!(diff.deletions, 0);
    }

    #[test]
    fn parse_numstat_no_trailing_newline() {
        let diff = parse_numstat("3\t1\tfile.rs");
        assert_eq!(diff.insertions, 3);
        assert_eq!(diff.deletions, 1);
    }
}
