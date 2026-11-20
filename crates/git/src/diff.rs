//! The diff of a single file, parsed from git's unified diff output.

use serde::Serialize;

use crate::review::{FileEntry, Status};

/// The diff of one file, as structured data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub status: Status,
    /// A binary file has no hunks.
    pub binary: bool,
    /// A submodule has no hunks; vugent does not look inside it.
    pub submodule: bool,
    /// The hunks hold every line of the file rather than a few lines of context around each change, which is the
    /// case unless the file is too large on either side (ADR 0010).
    pub whole: bool,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    pub(crate) fn empty(entry: &FileEntry) -> Self {
        Self {
            path: entry.path.clone(),
            old_path: entry.old_path.clone(),
            status: entry.status,
            binary: entry.binary,
            submodule: entry.submodule,
            whole: false,
            hunks: Vec::new(),
        }
    }
}

/// A run of changed lines with the unchanged lines around them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// The `@@ -1,2 +1,3 @@` line, including any function context git adds after it.
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

/// One line of a hunk, with its line number on each side it appears on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub kind: LineKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u32>,
    /// The line without its `+`, `-` or space prefix and without its line ending.
    pub text: String,
}

/// Parses the unified diff of a single file. Returns whether git called the file binary, and its hunks.
pub(crate) fn parse(output: &[u8]) -> (bool, Vec<Hunk>) {
    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut old_line, mut new_line) = (0, 0);
    for raw in output.split(|&byte| byte == b'\n') {
        let line = String::from_utf8_lossy(raw);
        let line = line.strip_suffix('\r').unwrap_or(&line);
        if let Some((old_start, new_start)) = hunk_start(line) {
            (old_line, new_line) = (old_start, new_start);
            hunks.push(Hunk {
                header: line.to_owned(),
                old_start,
                new_start,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            // The header before the first hunk: "diff --git", "index", "---", "+++" and friends.
            if line.starts_with("Binary files ") || line == "GIT binary patch" {
                return (true, Vec::new());
            }
            continue;
        };
        let (kind, text) = match line.split_at_checked(1) {
            Some(("+", text)) => (LineKind::Added, text),
            Some(("-", text)) => (LineKind::Removed, text),
            Some((" ", text)) => (LineKind::Context, text),
            // "\ No newline at end of file", or the empty string after the final newline.
            _ => continue,
        };
        let (old, new) = match kind {
            LineKind::Added => (None, Some(new_line)),
            LineKind::Removed => (Some(old_line), None),
            LineKind::Context => (Some(old_line), Some(new_line)),
        };
        old_line += u32::from(old.is_some());
        new_line += u32::from(new.is_some());
        hunk.lines.push(Line {
            kind,
            old_line: old,
            new_line: new,
            text: text.to_owned(),
        });
    }
    (false, hunks)
}

/// Shows the content of an unchanged file as a single hunk of context lines.
pub(crate) fn unchanged(content: &[u8]) -> Vec<Hunk> {
    let text = String::from_utf8_lossy(content);
    let lines: Vec<Line> = text
        .lines()
        .zip(1..)
        .map(|(text, number)| Line {
            kind: LineKind::Context,
            old_line: Some(number),
            new_line: Some(number),
            text: text.to_owned(),
        })
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let count = lines.len();
    vec![Hunk {
        header: format!("@@ -1,{count} +1,{count} @@"),
        old_start: 1,
        new_start: 1,
        lines,
    }]
}

/// Reads the start lines from a hunk header such as `@@ -10,3 +12,4 @@ fn main()`.
fn hunk_start(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let start = |range: &str| range.split(',').next()?.parse().ok();
    Some((start(old)?, start(new)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunks_with_line_numbers() {
        let output = b"diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n\
@@ -1,3 +1,3 @@ fn main()\n a\n-b\n+c\n d\n@@ -10 +10,2 @@\n x\n+y\n\\ No newline at end of file\n";

        let (binary, hunks) = parse(output);

        assert!(!binary);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].header, "@@ -1,3 +1,3 @@ fn main()");
        assert_eq!((hunks[1].old_start, hunks[1].new_start), (10, 10));
        let last = &hunks[1].lines[1];
        assert_eq!(
            (last.kind, last.old_line, last.new_line, last.text.as_str()),
            (LineKind::Added, None, Some(11), "y")
        );
        assert_eq!(hunks[1].lines.len(), 2);
    }

    #[test]
    fn removed_lines_that_look_like_headers_stay_in_the_hunk() {
        let (_, hunks) = parse(b"--- a/f\n+++ b/f\n@@ -1 +1 @@\n--- x\n+++ y\n");

        let kinds: Vec<_> = hunks[0].lines.iter().map(|line| line.kind).collect();
        assert_eq!(kinds, [LineKind::Removed, LineKind::Added]);
        assert_eq!(hunks[0].lines[0].text, "-- x");
    }

    #[test]
    fn detects_binary_files() {
        let (binary, hunks) = parse(b"diff --git a/f b/f\nBinary files a/f and b/f differ\n");

        assert!(binary);
        assert!(hunks.is_empty());
    }

    #[test]
    fn keeps_empty_lines_and_strips_carriage_returns() {
        let (_, hunks) = parse(b"@@ -1,2 +1,2 @@\n \n-a\r\n+b\r\n");

        let texts: Vec<_> = hunks[0]
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(texts, ["", "a", "b"]);
    }
}
