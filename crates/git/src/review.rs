//! Reviews of one folder of a repository, each a (base, target) pair from ADR 0002, ADR 0005 and ADR 0006:
//! uncommitted changes, staged or unstaged changes, a whole branch, or a single commit.

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
    /// Lines in the target of the review, counted the way git does: 0 for a deleted file, and 1 for a binary file, a
    /// symbolic link or a submodule.
    pub lines: u32,
}

/// Which review to show, as named in the API's `rev` parameter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Rev {
    /// Every uncommitted change, untracked files included, against `HEAD`.
    #[default]
    WorkTree,
    /// The staged changes: the index against `HEAD`.
    Index,
    /// The unstaged changes, untracked files included: the working tree against the index.
    Unstaged,
    /// Everything since the branch left the main branch: the working tree against the merge-base.
    Branch,
    /// A single commit against its first parent, given by its SHA.
    Commit(String),
}

impl Rev {
    /// Reads `worktree`, `index`, `unstaged`, `branch` or a commit's SHA in lowercase hex. Anything else is refused, so that no
    /// other text ever reaches git as a revision.
    pub fn parse(rev: &str) -> Option<Self> {
        match rev {
            "worktree" => Some(Self::WorkTree),
            "index" => Some(Self::Index),
            "unstaged" => Some(Self::Unstaged),
            "branch" => Some(Self::Branch),
            _ if (4..=64).contains(&rev.len())
                && rev
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
            {
                Some(Self::Commit(rev.to_owned()))
            }
            _ => None,
        }
    }
}

/// What a review compares the base against.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    WorkTree,
    Index,
    Commit(String),
}

/// A review resolved to concrete objects: `base` is a commit or the empty tree, or `None` for the index, which is
/// only compared against the working tree.
#[derive(Debug, Clone)]
struct Comparison {
    base: Option<String>,
    target: Target,
}

impl Comparison {
    /// The revision arguments of `git diff` that compare `base` against the target. Without a base, `git diff`
    /// compares the working tree against the index.
    fn range<'a>(&'a self, base: Option<&'a str>) -> Vec<&'a str> {
        match &self.target {
            Target::WorkTree => base.into_iter().collect(),
            Target::Index => ["--cached"].into_iter().chain(base).collect(),
            Target::Commit(commit) => base.into_iter().chain([commit.as_str()]).collect(),
        }
    }
}

/// git looks for a NUL byte in this many leading bytes to decide whether a file is binary.
const BINARY_PROBE: usize = 8000;

/// The branches tried, in order, when looking for the main branch.
const MAIN_BRANCHES: [&str; 3] = ["origin/HEAD", "main", "master"];

/// Reviews of one folder of a repository. Every call runs git again, so the result is always current. A review never
/// writes to the index or the working tree.
#[derive(Debug, Clone)]
pub struct Review {
    dir: PathBuf,
    /// The main branch given by the user, instead of looking for one.
    main: Option<String>,
}

impl Review {
    /// Reviews `dir`, which must be inside a git working tree. Only files under `dir` are included.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            main: None,
        }
    }

    /// Compares the branch against `main` instead of the first of `origin/HEAD`, `main` and `master` that exists.
    pub fn with_main(mut self, main: impl Into<String>) -> Self {
        self.main = Some(main.into());
        self
    }

    /// The reviewed folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The main branch that branch review compares against, or `None` when there is none.
    pub fn main_branch(&self) -> Result<Option<String>, Error> {
        let candidates = match &self.main {
            Some(main) => vec![main.as_str()],
            None => MAIN_BRANCHES.to_vec(),
        };
        for candidate in candidates {
            // A leading dash would be read as an option.
            if !candidate.starts_with('-') && self.commit(candidate)?.is_some() {
                return Ok(Some(candidate.to_owned()));
            }
        }
        Ok(None)
    }

    /// Lists every file under the reviewed folder in the target of `rev`, plus the files deleted since its base,
    /// sorted by path. Untracked (but not ignored) files are included when the target is the working tree.
    pub fn files(&self, rev: &Rev) -> Result<Vec<FileEntry>, Error> {
        let comparison = self.compare(rev)?;
        let mut files = BTreeMap::new();

        let listing = match &comparison.target {
            Target::Commit(commit) => run(&self.dir, &["ls-tree", "-r", "-z", commit, "--", "."])?,
            Target::WorkTree | Target::Index => {
                run(&self.dir, &["ls-files", "--stage", "-z", "--", "."])?
            }
        };
        for record in split_z(&listing) {
            // ls-files: <mode> <object> <stage>\t<path>; ls-tree: <mode> <type> <object>\t<path>
            let Some((meta, path)) = record.split_once('\t') else {
                continue;
            };
            let submodule = meta.starts_with("160000 ");
            files.insert(path.to_owned(), unchanged(path, submodule));
        }

        let range = comparison.range(comparison.base.as_deref());
        let counts = self.numstat(&range, true)?;
        let mut args = vec!["diff"];
        args.extend(&range);
        args.extend(["-M", "--name-status", "-z", "--relative", "--", "."]);
        let names = run(&self.dir, &args)?;
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

        if comparison.target == Target::WorkTree {
            for entry in self.untracked()? {
                files.insert(entry.path.clone(), entry);
            }
            self.count_work_tree_lines(&mut files)?;
        } else {
            // Diffing the empty tree against the target lists every file as added, with all of its lines.
            let empty = self.empty_tree()?;
            let lines = self.numstat(&comparison.range(Some(&empty)), false)?;
            for entry in files.values_mut() {
                entry.lines = match entry.status {
                    Status::Deleted => 0,
                    _ if entry.submodule || entry.binary => 1,
                    _ => match lines.get(&entry.path) {
                        Some(&(_, _, true)) => 1,
                        Some(&(added, _, false)) => added,
                        None => 0,
                    },
                };
            }
        }

        Ok(files.into_values().collect())
    }

    /// Returns the diff of one file of the review `rev`, given its path relative to the reviewed folder.
    pub fn diff(&self, rev: &Rev, path: &str) -> Result<FileDiff, Error> {
        let comparison = self.compare(rev)?;
        let entry = self
            .files(rev)?
            .into_iter()
            .find(|entry| entry.path == path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let mut diff = FileDiff::empty(&entry);
        if entry.submodule {
            return Ok(diff);
        }

        let output = match entry.status {
            Status::Unchanged => {
                let content = match &comparison.target {
                    Target::WorkTree => read(&self.dir.join(path))?,
                    // A path starting with ./ is relative to the reviewed folder, not the repository's root.
                    Target::Index => self.blob(&format!(":./{path}"))?,
                    Target::Commit(commit) => self.blob(&format!("{commit}:./{path}"))?,
                };
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
                let mut args = vec!["diff"];
                args.extend(comparison.range(comparison.base.as_deref()));
                args.extend([
                    "-M",
                    "--no-color",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--relative",
                    "--",
                ]);
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

    /// Resolves `rev` to the objects it compares.
    fn compare(&self, rev: &Rev) -> Result<Comparison, Error> {
        let head = || -> Result<Option<String>, Error> {
            match self.commit("HEAD")? {
                Some(head) => Ok(Some(head)),
                None => self.empty_tree().map(Some),
            }
        };
        Ok(match rev {
            Rev::WorkTree => Comparison {
                base: head()?,
                target: Target::WorkTree,
            },
            Rev::Index => Comparison {
                base: head()?,
                target: Target::Index,
            },
            Rev::Unstaged => Comparison {
                base: None,
                target: Target::WorkTree,
            },
            Rev::Branch => Comparison {
                base: Some(
                    self.branch_base()?
                        .ok_or_else(|| Error::NotFound("the branch".to_owned()))?,
                ),
                target: Target::WorkTree,
            },
            Rev::Commit(sha) => {
                let commit = self
                    .commit(sha)?
                    .ok_or_else(|| Error::NotFound(format!("commit {sha}")))?;
                let base = match self.commit(&format!("{commit}^1"))? {
                    Some(parent) => parent,
                    // The repository's first commit.
                    None => self.empty_tree()?,
                };
                Comparison {
                    base: Some(base),
                    target: Target::Commit(commit),
                }
            }
        })
    }

    /// The full SHA of the commit that `rev` names, or `None` when there is no such commit.
    pub(crate) fn commit(&self, rev: &str) -> Result<Option<String>, Error> {
        // --verify --quiet exits with 1 and no message when the revision does not exist, and 128 outside a
        // repository.
        let out = run_with(
            &self.dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{rev}^{{commit}}"),
            ],
            &[0, 1],
        )?;
        let sha = String::from_utf8_lossy(&out).trim().to_owned();
        Ok(Some(sha).filter(|sha| !sha.is_empty()))
    }

    /// Where the current branch left the main branch, or `None` when there is no main branch, no common history, or
    /// `HEAD` is on the main branch itself.
    pub(crate) fn branch_base(&self) -> Result<Option<String>, Error> {
        let (Some(head), Some(main)) = (self.commit("HEAD")?, self.main_branch()?) else {
            return Ok(None);
        };
        // merge-base exits with 1 when the two have no common history.
        let out = run_with(&self.dir, &["merge-base", &head, &main], &[0, 1])?;
        let base = String::from_utf8_lossy(&out).trim().to_owned();
        Ok(Some(base).filter(|base| !base.is_empty() && *base != head))
    }

    /// The empty tree, the base when there is no commit to compare against.
    pub(crate) fn empty_tree(&self) -> Result<String, Error> {
        // Hashing without -w writes nothing. Its stdin is empty, which is the empty tree, in SHA-1 and SHA-256 repos.
        let empty = run(&self.dir, &["hash-object", "-t", "tree", "--stdin"])?;
        Ok(String::from_utf8_lossy(&empty).trim().to_owned())
    }

    /// Untracked files that are not ignored, each shown as added.
    fn untracked(&self) -> Result<Vec<FileEntry>, Error> {
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
        split_z(&others)
            .into_iter()
            .map(|path| {
                Ok(match path.strip_suffix('/') {
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
                })
            })
            .collect()
    }

    /// Counts the lines of every tracked file in the working tree. Untracked files are counted when they are listed.
    fn count_work_tree_lines(&self, files: &mut BTreeMap<String, FileEntry>) -> Result<(), Error> {
        for entry in files.values_mut().filter(|entry| !entry.untracked) {
            entry.lines = match entry.status {
                Status::Deleted => 0,
                _ if entry.submodule || entry.binary => 1,
                _ => match count_lines(&self.dir.join(&entry.path)) {
                    Ok((_, true)) => 1,
                    Ok((lines, false)) => lines,
                    // Added to the index and then deleted: git shows no change against HEAD. Against the index it is
                    // deleted, and handled above.
                    Err(Error::Read { source, .. }) if source.kind() == ErrorKind::NotFound => 0,
                    Err(err) => return Err(err),
                },
            };
        }
        Ok(())
    }

    /// The content of a blob, such as `:./src/lib.rs` in the index or `<commit>:./src/lib.rs`.
    fn blob(&self, object: &str) -> Result<Vec<u8>, Error> {
        run(&self.dir, &["cat-file", "blob", object])
    }

    /// Line counts of changed tracked files between the revisions of `range`, keyed by their path:
    /// (added, removed, binary).
    fn numstat(
        &self,
        range: &[&str],
        renames: bool,
    ) -> Result<HashMap<String, (u32, u32, bool)>, Error> {
        let mut args = vec!["diff"];
        args.extend(range);
        args.extend([
            if renames { "-M" } else { "--no-renames" },
            "--numstat",
            "-z",
            "--relative",
            "--",
            ".",
        ]);
        let out = run(&self.dir, &args)?;
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
pub(crate) fn split_z(out: &[u8]) -> Vec<&str> {
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

        let files = Review::new(tmp.path()).files(&Rev::WorkTree).unwrap();

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

        let files = Review::new(dir).files(&Rev::WorkTree).unwrap();

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

        let files = Review::new(dir).files(&Rev::WorkTree).unwrap();

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

        let files = Review::new(tmp.path()).files(&Rev::WorkTree).unwrap();

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

        let files = Review::new(tmp.path().join("src"))
            .files(&Rev::WorkTree)
            .unwrap();

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
        let files = review.files(&Rev::WorkTree).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|entry| entry.status == Status::Added));
        let diff = review.diff(&Rev::WorkTree, "staged.txt").unwrap();
        assert_eq!(kinds(&diff), [(LineKind::Added, "s")]);
    }

    #[test]
    fn works_on_a_detached_head() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "--detach"]);
        write(tmp.path(), "a.txt", b"one\n");

        let files = Review::new(tmp.path()).files(&Rev::WorkTree).unwrap();

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
        let files = review.files(&Rev::WorkTree).unwrap();

        let lib = find(&files, "vendor/lib");
        assert!(lib.submodule && !lib.binary);
        assert_eq!(lib.status, Status::Added);
        let nested = find(&files, "nested");
        assert!(nested.submodule && nested.untracked);
        assert_eq!((lib.lines, nested.lines), (1, 1));
        assert!(!files.iter().any(|entry| entry.path.starts_with("nested/")));
        let diff = review.diff(&Rev::WorkTree, "vendor/lib").unwrap();
        assert!(diff.submodule && diff.hunks.is_empty());
    }

    #[test]
    fn diffs_a_modified_file() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"one\n2\nthree\n");

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "a.txt")
            .unwrap();

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

        let added = review.diff(&Rev::WorkTree, "src/new.txt").unwrap();
        let deleted = review.diff(&Rev::WorkTree, "a.txt").unwrap();

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

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "src/new.rs")
            .unwrap();

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

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "image.bin")
            .unwrap();

        assert!(diff.binary);
        assert!(diff.hunks.is_empty());
    }

    #[test]
    fn unchanged_files_are_shown_as_context() {
        let tmp = repo();

        let diff = Review::new(tmp.path().join("src"))
            .diff(&Rev::WorkTree, "lib.rs")
            .unwrap();

        assert_eq!(kinds(&diff), [(LineKind::Context, "fn main() {}")]);
    }

    #[test]
    fn paths_outside_the_review_are_not_found() {
        let tmp = repo();

        let err = Review::new(tmp.path().join("src"))
            .diff(&Rev::WorkTree, "../a.txt")
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
        review.files(&Rev::WorkTree).unwrap();
        review.diff(&Rev::WorkTree, "a.txt").unwrap();
        review.diff(&Rev::WorkTree, "new.txt").unwrap();

        assert_eq!(std::fs::read(&index).unwrap(), before);
    }

    fn head(dir: &Path) -> String {
        git(dir, &["rev-parse", "HEAD"]).trim().to_owned()
    }

    #[test]
    fn parses_revs() {
        assert_eq!(Rev::parse("worktree"), Some(Rev::WorkTree));
        assert_eq!(Rev::parse("index"), Some(Rev::Index));
        assert_eq!(Rev::parse("unstaged"), Some(Rev::Unstaged));
        assert_eq!(Rev::parse("branch"), Some(Rev::Branch));
        assert_eq!(
            Rev::parse("0123abcd"),
            Some(Rev::Commit("0123abcd".to_owned()))
        );
        for bad in [
            "",
            "HEAD",
            "abc",
            "--all",
            "ABCDEF12",
            "abc1234^",
            &"a".repeat(65),
        ] {
            assert_eq!(Rev::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_index_shows_only_staged_changes() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\n");
        git(dir, &["add", "a.txt"]);
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\nfive\n");
        write(dir, "src/lib.rs", b"fn main() { }\n");
        write(dir, "new.txt", b"n\n");

        let review = Review::new(dir);
        let files = review.files(&Rev::Index).unwrap();

        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.lines),
            (Status::Modified, 1, 0, 4)
        );
        assert_eq!(find(&files, "src/lib.rs").status, Status::Unchanged);
        assert!(!files.iter().any(|entry| entry.path == "new.txt"));
        let diff = review.diff(&Rev::Index, "a.txt").unwrap();
        assert_eq!(kinds(&diff).last(), Some(&(LineKind::Added, "four")));
        let lib = review.diff(&Rev::Index, "src/lib.rs").unwrap();
        assert_eq!(kinds(&lib), [(LineKind::Context, "fn main() {}")]);
    }

    #[test]
    fn unstaged_shows_only_changes_since_the_index() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\n");
        git(dir, &["add", "a.txt"]);
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\nfive\n");
        write(dir, "staged.txt", b"s\n");
        git(dir, &["add", "staged.txt"]);
        write(dir, "new.txt", b"n\n");
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();

        let review = Review::new(dir);
        let files = review.files(&Rev::Unstaged).unwrap();

        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.lines),
            (Status::Modified, 1, 0, 5)
        );
        let staged = find(&files, "staged.txt");
        assert_eq!((staged.status, staged.lines), (Status::Unchanged, 1));
        let new = find(&files, "new.txt");
        assert_eq!((new.status, new.untracked), (Status::Added, true));
        assert_eq!(find(&files, "src/lib.rs").status, Status::Deleted);
        let diff = review.diff(&Rev::Unstaged, "a.txt").unwrap();
        assert_eq!(
            kinds(&diff),
            [
                (LineKind::Context, "two"),
                (LineKind::Context, "three"),
                (LineKind::Context, "four"),
                (LineKind::Added, "five"),
            ]
        );
        let unchanged = review.diff(&Rev::Unstaged, "staged.txt").unwrap();
        assert_eq!(kinds(&unchanged), [(LineKind::Context, "s")]);
    }

    #[test]
    fn unstaged_works_in_a_repo_without_commits() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "staged.txt", b"s\n");
        git(tmp.path(), &["add", "staged.txt"]);
        write(tmp.path(), "loose.txt", b"l\n");

        let files = Review::new(tmp.path()).files(&Rev::Unstaged).unwrap();

        assert_eq!(find(&files, "staged.txt").status, Status::Unchanged);
        assert_eq!(find(&files, "loose.txt").status, Status::Added);
    }

    #[test]
    fn a_commit_shows_the_tree_as_it_was_and_only_its_changes() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\n2\nthree\n");
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "--quiet", "-m", "change"]);
        let change = head(dir);
        write(dir, "later.txt", b"later\n");
        write(dir, "src/old.rs", b"rewritten\n");
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "--quiet", "-m", "later"]);
        write(dir, "a.txt", b"uncommitted\n");

        let review = Review::new(dir);
        let rev = Rev::Commit(change);
        let files = review.files(&rev).unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "src/lib.rs", "src/old.rs"]);
        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.lines),
            (Status::Modified, 1, 1, 3)
        );
        let lib = find(&files, "src/lib.rs");
        assert_eq!((lib.status, lib.lines), (Status::Deleted, 0));
        let old = find(&files, "src/old.rs");
        assert_eq!((old.status, old.lines), (Status::Unchanged, 3));
        let diff = review.diff(&rev, "src/old.rs").unwrap();
        assert_eq!(
            kinds(&diff)[0],
            (LineKind::Context, "// a file long enough")
        );
        let diff = review.diff(&rev, "a.txt").unwrap();
        assert!(kinds(&diff).contains(&(LineKind::Added, "2")));
    }

    #[test]
    fn the_first_commit_is_all_added() {
        let tmp = repo();
        let first = head(tmp.path());

        let files = Review::new(tmp.path()).files(&Rev::Commit(first)).unwrap();

        assert!(files.iter().all(|entry| entry.status == Status::Added));
        assert_eq!(find(&files, "a.txt").added, 3);
    }

    #[test]
    fn a_commit_works_in_a_subfolder() {
        let tmp = repo();
        let first = head(tmp.path());

        let review = Review::new(tmp.path().join("src"));
        let files = review.files(&Rev::Commit(first.clone())).unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["lib.rs", "old.rs"]);
        assert_eq!(find(&files, "lib.rs").lines, 1);
        let diff = review.diff(&Rev::Commit(first), "lib.rs").unwrap();
        assert_eq!(kinds(&diff), [(LineKind::Added, "fn main() {}")]);
    }

    #[test]
    fn a_merge_commit_is_compared_against_its_first_parent() {
        let tmp = repo();
        let dir = tmp.path();
        git(dir, &["checkout", "--quiet", "-b", "side"]);
        write(dir, "side.txt", b"s\n");
        git(dir, &["add", "side.txt"]);
        git(dir, &["commit", "--quiet", "-m", "side"]);
        git(dir, &["checkout", "--quiet", "main"]);
        write(dir, "main.txt", b"m\n");
        git(dir, &["add", "main.txt"]);
        git(dir, &["commit", "--quiet", "-m", "main"]);
        git(dir, &["merge", "--quiet", "--no-ff", "-m", "merge", "side"]);

        let files = Review::new(dir).files(&Rev::Commit(head(dir))).unwrap();

        assert_eq!(find(&files, "side.txt").status, Status::Added);
        assert_eq!(find(&files, "main.txt").status, Status::Unchanged);
    }

    #[test]
    fn an_unknown_commit_is_not_found() {
        let tmp = repo();

        let err = Review::new(tmp.path())
            .files(&Rev::Commit("0".repeat(40)))
            .unwrap_err();

        assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    }

    #[test]
    fn the_branch_spans_its_commits_and_uncommitted_work() {
        let tmp = repo();
        let dir = tmp.path();
        git(dir, &["checkout", "--quiet", "-b", "feature"]);
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\n");
        git(dir, &["commit", "--quiet", "-am", "four"]);
        write(dir, "new.txt", b"n\n");

        let review = Review::new(dir);
        let files = review.files(&Rev::Branch).unwrap();

        assert_eq!(find(&files, "a.txt").status, Status::Modified);
        assert_eq!(find(&files, "new.txt").status, Status::Added);
        git(dir, &["checkout", "--quiet", "main"]);
        let err = review.files(&Rev::Branch).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    }
}
