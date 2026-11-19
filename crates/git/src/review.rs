//! The review from ADR 0002: every uncommitted change in a folder, untracked files included, compared against `HEAD`.

use std::collections::{BTreeMap, HashMap};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diff::{self, FileDiff};
use crate::{Error, run, run_with};

/// How a file changed between the base and the target of a review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
    Unchanged,
}

/// A file in the reviewed folder, changed or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// The path relative to the reviewed folder, with `/` separators.
    pub path: String,
    pub status: Status,
    /// Lines added; 0 for binary files and submodules.
    pub added: u32,
    /// Lines removed; 0 for binary files and submodules.
    pub removed: u32,
    pub binary: bool,
    /// The path before a rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// A submodule, or an untracked nested repository, shown as a single entry.
    pub submodule: bool,
    /// Not tracked by git yet.
    pub untracked: bool,
    /// Lines in the working tree, counted the way git does: 0 for a deleted file, and 1 for a binary file, a symbolic
    /// link or a submodule.
    pub lines: u32,
}

/// git looks for a NUL byte in this many leading bytes to decide whether a file is binary.
const BINARY_PROBE: usize = 8000;

/// A review of the uncommitted changes in one folder of a repository. Every call runs git again, so the result is
/// always current. A review never writes to the index or the working tree.
#[derive(Debug, Clone)]
pub struct Review {
    dir: PathBuf,
}

impl Review {
    /// Reviews `dir`, which must be inside a git working tree. Only files under `dir` are included.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The reviewed folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Lists every file under the reviewed folder: tracked, untracked (but not ignored) and deleted, sorted by path.
    pub fn files(&self) -> Result<Vec<FileEntry>, Error> {
        let base = self.base()?;
        let mut files = BTreeMap::new();

        let staged = run(&self.dir, &["ls-files", "--stage", "-z", "--", "."])?;
        for record in split_z(&staged) {
            // <mode> <object> <stage>\t<path>
            let Some((meta, path)) = record.split_once('\t') else {
                continue;
            };
            let submodule = meta.starts_with("160000 ");
            files.insert(path.to_owned(), unchanged(path, submodule));
        }

        let counts = self.numstat(&base)?;
        let names = run(
            &self.dir,
            &[
                "diff",
                &base,
                "-M",
                "--name-status",
                "-z",
                "--relative",
                "--",
                ".",
            ],
        )?;
        let mut names = split_z(&names).into_iter();
        while let Some(code) = names.next() {
            let (status, old_path) = match code.as_bytes().first() {
                Some(b'A') => (Status::Added, None),
                Some(b'D') => (Status::Deleted, None),
                Some(b'R') => (Status::Renamed, names.next()),
                // C (copied) only appears with -C, but its record has two paths as well.
                Some(b'C') => (Status::Added, names.next()),
                _ => (Status::Modified, None),
            };
            let Some(path) = names.next() else { break };
            if let (Status::Renamed, Some(old)) = (status, old_path) {
                files.remove(old);
            }
            let (added, removed, binary) = counts.get(path).copied().unwrap_or_default();
            let entry = files
                .entry(path.to_owned())
                .or_insert_with(|| unchanged(path, false));
            entry.status = status;
            if !entry.submodule {
                entry.added = added;
                entry.removed = removed;
                entry.binary = binary;
            }
            entry.old_path = old_path
                .filter(|_| status == Status::Renamed)
                .map(str::to_owned);
        }

        let others = run(
            &self.dir,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                ".",
            ],
        )?;
        for path in split_z(&others) {
            let entry = match path.strip_suffix('/') {
                // git lists an untracked nested repository as a folder and does not look inside it.
                Some(repo) => FileEntry {
                    status: Status::Added,
                    untracked: true,
                    lines: 1,
                    ..unchanged(repo, true)
                },
                None => {
                    let (lines, binary) = count_lines(&self.dir.join(path))?;
                    FileEntry {
                        status: Status::Added,
                        added: lines,
                        binary,
                        untracked: true,
                        lines: if binary { 1 } else { lines },
                        ..unchanged(path, false)
                    }
                }
            };
            files.insert(entry.path.clone(), entry);
        }

        for entry in files.values_mut().filter(|entry| !entry.untracked) {
            entry.lines = match entry.status {
                Status::Deleted => 0,
                _ if entry.submodule || entry.binary => 1,
                _ => match count_lines(&self.dir.join(&entry.path)) {
                    Ok((_, true)) => 1,
                    Ok((lines, false)) => lines,
                    // Added to the index and then deleted: git shows no change against HEAD.
                    Err(Error::Read { source, .. }) if source.kind() == ErrorKind::NotFound => 0,
                    Err(err) => return Err(err),
                },
            };
        }

        Ok(files.into_values().collect())
    }

    /// Returns the diff of one file of the review, given its path relative to the reviewed folder.
    pub fn diff(&self, path: &str) -> Result<FileDiff, Error> {
        let entry = self
            .files()?
            .into_iter()
            .find(|entry| entry.path == path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let mut diff = FileDiff::empty(&entry);
        if entry.submodule {
            return Ok(diff);
        }

        let output = match entry.status {
            Status::Unchanged => {
                let content = read(&self.dir.join(path))?;
                diff.binary = is_binary(&content);
                if !diff.binary {
                    diff.hunks = diff::unchanged(&content);
                }
                return Ok(diff);
            }
            _ if entry.untracked => run_with(
                &self.dir,
                &[
                    "diff",
                    "--no-index",
                    "--no-color",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--",
                    "/dev/null",
                    path,
                ],
                // With --no-index, 1 means the files differ.
                &[0, 1],
            )?,
            _ => {
                let base = self.base()?;
                let mut args = vec![
                    "diff",
                    &base,
                    "-M",
                    "--no-color",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--relative",
                    "--",
                ];
                args.extend(entry.old_path.as_deref());
                args.push(path);
                run(&self.dir, &args)?
            }
        };
        let (binary, hunks) = diff::parse(&output);
        diff.binary = binary;
        diff.hunks = hunks;
        Ok(diff)
    }

    /// The commit to compare against: `HEAD`, or the empty tree on a branch with no commits yet.
    fn base(&self) -> Result<String, Error> {
        // --verify --quiet exits with 1 and no message when HEAD does not exist yet, and 128 outside a repository.
        let head = run_with(
            &self.dir,
            &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
            &[0, 1],
        )?;
        let head = String::from_utf8_lossy(&head).trim().to_owned();
        if !head.is_empty() {
            return Ok(head);
        }
        // Hashing without -w writes nothing. Its stdin is empty, which is the empty tree, in SHA-1 and SHA-256 repos.
        let empty = run(&self.dir, &["hash-object", "-t", "tree", "--stdin"])?;
        Ok(String::from_utf8_lossy(&empty).trim().to_owned())
    }

    /// Line counts of changed tracked files, keyed by their path: (added, removed, binary).
    fn numstat(&self, base: &str) -> Result<HashMap<String, (u32, u32, bool)>, Error> {
        let out = run(
            &self.dir,
            &[
                "diff",
                base,
                "-M",
                "--numstat",
                "-z",
                "--relative",
                "--",
                ".",
            ],
        )?;
        let mut counts = HashMap::new();
        let mut records = split_z(&out).into_iter();
        while let Some(record) = records.next() {
            // <added>\t<removed>\t<path>, or <added>\t<removed>\t followed by the old and new paths for a rename.
            let mut fields = record.splitn(3, '\t');
            let (Some(added), Some(removed), Some(path)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let path = if path.is_empty() {
                records.next();
                match records.next() {
                    Some(new) => new,
                    None => break,
                }
            } else {
                path
            };
            let binary = added == "-" && removed == "-";
            counts.insert(
                path.to_owned(),
                (
                    added.parse().unwrap_or(0),
                    removed.parse().unwrap_or(0),
                    binary,
                ),
            );
        }
        Ok(counts)
    }
}

fn unchanged(path: &str, submodule: bool) -> FileEntry {
    FileEntry {
        path: path.to_owned(),
        status: Status::Unchanged,
        added: 0,
        removed: 0,
        binary: false,
        old_path: None,
        submodule,
        untracked: false,
        lines: 0,
    }
}

/// Splits NUL-terminated output, as printed by git's `-z` options, into its records.
fn split_z(out: &[u8]) -> Vec<&str> {
    out.split(|&byte| byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| std::str::from_utf8(record).unwrap_or_default())
        .filter(|record| !record.is_empty())
        .collect()
}

pub(crate) fn is_binary(content: &[u8]) -> bool {
    content[..content.len().min(BINARY_PROBE)].contains(&0)
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    // A symbolic link is stored by git as its target, so show the target rather than following it.
    if let Ok(target) = std::fs::read_link(path) {
        return Ok(target.to_string_lossy().into_owned().into_bytes());
    }
    std::fs::read(path).map_err(|source| Error::Read {
        path: path.display().to_string(),
        source,
    })
}

/// Counts the lines of a working tree file the way `git diff` would, and tells whether it is binary.
fn count_lines(path: &Path) -> Result<(u32, bool), Error> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Ok((1, false));
    }
    let error = |source| Error::Read {
        path: path.display().to_string(),
        source,
    };
    let mut file = std::fs::File::open(path).map_err(error)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content).map_err(error)?;
    if is_binary(&content) {
        return Ok((0, true));
    }
    let newlines = content.iter().filter(|&&byte| byte == b'\n').count();
    let unterminated = usize::from(content.last().is_some_and(|&byte| byte != b'\n'));
    Ok((
        u32::try_from(newlines + unterminated).unwrap_or(u32::MAX),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{git, write};
    use crate::{FileDiff, LineKind};

    /// A repository with one commit: `a.txt`, `src/lib.rs` and `src/old.rs`.
    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        git(dir, &["init", "--quiet"]);
        write(dir, "a.txt", b"one\ntwo\nthree\n");
        write(dir, "src/lib.rs", b"fn main() {}\n");
        write(
            dir,
            "src/old.rs",
            b"// a file long enough\n// to be found\n// as renamed\n",
        );
        git(dir, &["add", "."]);
        git(dir, &["commit", "--quiet", "-m", "init"]);
        tmp
    }

    fn find<'a>(files: &'a [FileEntry], path: &str) -> &'a FileEntry {
        files
            .iter()
            .find(|entry| entry.path == path)
            .unwrap_or_else(|| panic!("{path} missing from {files:#?}"))
    }

    fn kinds(diff: &FileDiff) -> Vec<(LineKind, &str)> {
        diff.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .map(|line| (line.kind, line.text.as_str()))
            .collect()
    }

    #[test]
    fn a_clean_repo_lists_every_file_as_unchanged() {
        let tmp = repo();

        let files = Review::new(tmp.path()).files().unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "src/lib.rs", "src/old.rs"]);
        assert!(files.iter().all(|entry| entry.status == Status::Unchanged));
    }

    #[test]
    fn lists_every_kind_of_change() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\n2\nthree\nfour\n");
        write(dir, "new.txt", b"x\ny");
        write(dir, "staged.txt", b"s\n");
        git(dir, &["add", "staged.txt"]);
        git(dir, &["mv", "src/old.rs", "src/new.rs"]);
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();
        write(dir, "image.bin", b"\0\x01\x02");
        write(dir, "ignored.log", b"noise\n");
        write(dir, ".gitignore", b"*.log\n");

        let files = Review::new(dir).files().unwrap();

        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.untracked),
            (Status::Modified, 2, 1, false)
        );
        let new = find(&files, "new.txt");
        assert_eq!(
            (new.status, new.added, new.untracked),
            (Status::Added, 2, true)
        );
        assert_eq!(find(&files, "staged.txt").status, Status::Added);
        let renamed = find(&files, "src/new.rs");
        assert_eq!(renamed.status, Status::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("src/old.rs"));
        assert!(!files.iter().any(|entry| entry.path == "src/old.rs"));
        assert_eq!(find(&files, "src/lib.rs").status, Status::Deleted);
        let image = find(&files, "image.bin");
        assert!(image.binary);
        assert_eq!(image.added, 0);
        assert!(!files.iter().any(|entry| entry.path == "ignored.log"));
    }

    #[test]
    fn counts_the_lines_of_every_file_in_the_working_tree() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\ntwo");
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();
        write(dir, "new.txt", b"x\ny\nz\n");
        write(dir, "image.bin", b"\0\x01\x02");
        write(dir, "gone.txt", b"g\n");
        git(dir, &["add", "gone.txt"]);
        std::fs::remove_file(dir.join("gone.txt")).unwrap();

        let files = Review::new(dir).files().unwrap();

        let lines = |path| find(&files, path).lines;
        assert_eq!(lines("a.txt"), 2);
        assert_eq!(lines("src/lib.rs"), 0);
        assert_eq!(lines("src/old.rs"), 3);
        assert_eq!(lines("new.txt"), 3);
        assert_eq!(lines("image.bin"), 1);
        assert_eq!(lines("gone.txt"), 0);
    }

    #[test]
    fn binary_tracked_files_have_no_line_counts() {
        let tmp = repo();
        write(tmp.path(), "blob.bin", b"\0a");
        git(tmp.path(), &["add", "blob.bin"]);
        git(tmp.path(), &["commit", "--quiet", "-m", "blob"]);
        write(tmp.path(), "blob.bin", b"\0b");

        let files = Review::new(tmp.path()).files().unwrap();

        let blob = find(&files, "blob.bin");
        assert_eq!(
            (blob.status, blob.binary, blob.added, blob.removed),
            (Status::Modified, true, 0, 0)
        );
    }

    #[test]
    fn is_limited_to_the_reviewed_folder() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"changed\n");
        write(tmp.path(), "src/extra.rs", b"\n");

        let files = Review::new(tmp.path().join("src")).files().unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["extra.rs", "lib.rs", "old.rs"]);
    }

    #[test]
    fn a_repo_without_commits_shows_everything_as_added() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "staged.txt", b"s\n");
        git(tmp.path(), &["add", "staged.txt"]);
        write(tmp.path(), "loose.txt", b"l\n");

        let review = Review::new(tmp.path());
        let files = review.files().unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|entry| entry.status == Status::Added));
        let diff = review.diff("staged.txt").unwrap();
        assert_eq!(kinds(&diff), [(LineKind::Added, "s")]);
    }

    #[test]
    fn works_on_a_detached_head() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "--detach"]);
        write(tmp.path(), "a.txt", b"one\n");

        let files = Review::new(tmp.path()).files().unwrap();

        assert_eq!(find(&files, "a.txt").status, Status::Modified);
    }

    #[test]
    fn submodules_are_single_entries() {
        let tmp = repo();
        let dir = tmp.path();
        // Adding a checked-out repository records it as a submodule (a gitlink), as `git submodule add` does.
        let lib = dir.join("vendor/lib");
        std::fs::create_dir_all(&lib).unwrap();
        git(&lib, &["init", "--quiet"]);
        write(&lib, "code.rs", b"\n");
        git(&lib, &["add", "."]);
        git(&lib, &["commit", "--quiet", "-m", "lib"]);
        git(dir, &["add", "vendor/lib"]);
        let nested = dir.join("nested");
        std::fs::create_dir(&nested).unwrap();
        git(&nested, &["init", "--quiet"]);
        write(&nested, "inside.txt", b"hidden\n");

        let review = Review::new(dir);
        let files = review.files().unwrap();

        let lib = find(&files, "vendor/lib");
        assert!(lib.submodule && !lib.binary);
        assert_eq!(lib.status, Status::Added);
        let nested = find(&files, "nested");
        assert!(nested.submodule && nested.untracked);
        assert_eq!((lib.lines, nested.lines), (1, 1));
        assert!(!files.iter().any(|entry| entry.path.starts_with("nested/")));
        let diff = review.diff("vendor/lib").unwrap();
        assert!(diff.submodule && diff.hunks.is_empty());
    }

    #[test]
    fn diffs_a_modified_file() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"one\n2\nthree\n");

        let diff = Review::new(tmp.path()).diff("a.txt").unwrap();

        assert_eq!(diff.status, Status::Modified);
        assert!(!diff.binary);
        assert_eq!(
            kinds(&diff),
            [
                (LineKind::Context, "one"),
                (LineKind::Removed, "two"),
                (LineKind::Added, "2"),
                (LineKind::Context, "three"),
            ]
        );
        let lines = &diff.hunks[0].lines;
        assert_eq!((lines[1].old_line, lines[1].new_line), (Some(2), None));
        assert_eq!((lines[2].old_line, lines[2].new_line), (None, Some(2)));
        assert_eq!((lines[3].old_line, lines[3].new_line), (Some(3), Some(3)));
    }

    #[test]
    fn diffs_untracked_and_deleted_files_in_full() {
        let tmp = repo();
        write(tmp.path(), "src/new.txt", b"a\nb\n");
        std::fs::remove_file(tmp.path().join("a.txt")).unwrap();
        let review = Review::new(tmp.path());

        let added = review.diff("src/new.txt").unwrap();
        let deleted = review.diff("a.txt").unwrap();

        assert_eq!(
            kinds(&added),
            [(LineKind::Added, "a"), (LineKind::Added, "b")]
        );
        assert_eq!(
            kinds(&deleted),
            [
                (LineKind::Removed, "one"),
                (LineKind::Removed, "two"),
                (LineKind::Removed, "three"),
            ]
        );
    }

    #[test]
    fn diffs_a_renamed_file_against_its_old_path() {
        let tmp = repo();
        git(tmp.path(), &["mv", "src/old.rs", "src/new.rs"]);
        write(
            tmp.path(),
            "src/new.rs",
            b"// a file long enough\n// to be found\n// as renamed!\n",
        );

        let diff = Review::new(tmp.path()).diff("src/new.rs").unwrap();

        assert_eq!(diff.status, Status::Renamed);
        assert_eq!(diff.old_path.as_deref(), Some("src/old.rs"));
        assert_eq!(
            kinds(&diff),
            [
                (LineKind::Context, "// a file long enough"),
                (LineKind::Context, "// to be found"),
                (LineKind::Removed, "// as renamed"),
                (LineKind::Added, "// as renamed!"),
            ]
        );
    }

    #[test]
    fn binary_files_have_no_lines() {
        let tmp = repo();
        write(tmp.path(), "image.bin", b"\0\x01");

        let diff = Review::new(tmp.path()).diff("image.bin").unwrap();

        assert!(diff.binary);
        assert!(diff.hunks.is_empty());
    }

    #[test]
    fn unchanged_files_are_shown_as_context() {
        let tmp = repo();

        let diff = Review::new(tmp.path().join("src")).diff("lib.rs").unwrap();

        assert_eq!(kinds(&diff), [(LineKind::Context, "fn main() {}")]);
    }

    #[test]
    fn paths_outside_the_review_are_not_found() {
        let tmp = repo();

        let err = Review::new(tmp.path().join("src"))
            .diff("../a.txt")
            .unwrap_err();

        assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    }

    #[test]
    fn never_writes_to_the_index() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"changed\n");
        write(tmp.path(), "new.txt", b"new\n");
        // Make the index stat data stale, which tempts git into refreshing it.
        write(tmp.path(), "src/lib.rs", b"fn main() {}\n");
        let index = tmp.path().join(".git/index");
        let before = std::fs::read(&index).unwrap();

        let review = Review::new(tmp.path());
        review.files().unwrap();
        review.diff("a.txt").unwrap();
        review.diff("new.txt").unwrap();

        assert_eq!(std::fs::read(&index).unwrap(), before);
    }
}
