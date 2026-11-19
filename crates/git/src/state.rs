//! What a file watcher needs from git (ADR 0008): where the git directories are, and, when files cannot be watched,
//! a snapshot of the reviewed folder's state to poll.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::review::Review;
use crate::{Error, run, run_with};

/// The git directories of a working tree. In a linked worktree `HEAD` and `index` are in the worktree's own git
/// directory and the refs in the common one; otherwise both are the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDirs {
    /// Holds `HEAD` and `index`.
    pub git_dir: PathBuf,
    /// Holds `refs/` and `packed-refs`.
    pub common_dir: PathBuf,
}

/// What a poll compares with the previous one. Each part is opaque; only whether it changed matters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Changes in the working tree: each file that differs from the index or is untracked, with its size and
    /// modification time, since editing an already modified file again does not change what `git status` prints.
    pub files: Vec<u8>,
    /// What is staged, from the index columns of `git status`.
    pub index: Vec<u8>,
    /// Where `HEAD` points and every ref.
    pub refs: Vec<u8>,
}

impl Review {
    /// Finds the git directories of the reviewed folder's working tree, as absolute paths.
    pub fn git_dirs(&self) -> Result<GitDirs, Error> {
        let out = run(
            self.dir(),
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-dir",
                "--git-common-dir",
            ],
        )?;
        let out = String::from_utf8_lossy(&out);
        let mut lines = out.lines();
        match (lines.next(), lines.next()) {
            (Some(git_dir), Some(common_dir)) => Ok(GitDirs {
                git_dir: PathBuf::from(git_dir),
                common_dir: PathBuf::from(common_dir),
            }),
            _ => Err(Error::Failed {
                args: "rev-parse --git-dir --git-common-dir".to_owned(),
                stderr: format!("unexpected output {out:?}"),
            }),
        }
    }

    /// Reads the state that polling compares. Like every review, it never writes to the index.
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        let status = run(
            self.dir(),
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--untracked-files=all",
                "--no-renames",
                "--",
                ".",
            ],
        )?;
        let mut snapshot = Snapshot::default();
        let root = self.toplevel()?;
        for entry in status.split(|&byte| byte == 0) {
            let Some((index, path)) = parse_status(entry) else {
                continue;
            };
            if let Some(index) = index {
                snapshot.index.extend_from_slice(index);
                snapshot.index.push(0);
            }
            if let Some(path) = path {
                snapshot.files.extend_from_slice(path);
                snapshot.files.push(0);
                let stamp = stamp(&root.join(String::from_utf8_lossy(path).as_ref()));
                snapshot.files.extend_from_slice(stamp.as_bytes());
            }
        }

        // An unborn branch has no HEAD commit, so rev-parse prints HEAD itself and exits with 128 without --verify.
        snapshot.refs = run_with(self.dir(), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
        snapshot.refs.extend(run_with(
            self.dir(),
            &["rev-parse", "--verify", "--quiet", "HEAD"],
            &[0, 1],
        )?);
        snapshot.refs.extend(run(
            self.dir(),
            &["for-each-ref", "--format=%(objectname) %(refname)"],
        )?);
        Ok(snapshot)
    }

    fn toplevel(&self) -> Result<PathBuf, Error> {
        crate::repo_root(self.dir())
    }
}

/// One entry of `git status`, split into its two sides: what it says about the index, and the path of a file that
/// differs from the index or is untracked.
type StatusSides<'a> = (Option<&'a [u8]>, Option<&'a [u8]>);

/// Reads one NUL-separated entry of `git status --porcelain=v2 -z`. Paths are relative to the top of the working tree.
fn parse_status(entry: &[u8]) -> Option<StatusSides<'_>> {
    match entry.first()? {
        b'?' => Some((None, entry.get(2..))),
        // 1 XY sub mH mI mW hH hI path, and u XY sub m1 m2 m3 mW h1 h2 h3 path.
        kind @ (b'1' | b'u') => {
            let fields = if *kind == b'1' { 8 } else { 10 };
            let mut parts = entry.splitn(fields + 1, |&byte| byte == b' ');
            let xy = parts.nth(1)?;
            let path = parts.nth(fields - 2)?;
            // The index side: X, and the path with the object names, which change when the file is staged again.
            let index = (xy.first() != Some(&b'.')).then_some(entry);
            let worktree = (xy.get(1) != Some(&b'.')).then_some(path);
            Some((index, worktree))
        }
        _ => None,
    }
}

/// The size and modification time of the file at `path`, or nothing when it cannot be read.
fn stamp(path: &Path) -> String {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return "-\0".to_owned();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |time| time.as_nanos());
    format!("{} {modified}\0", metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{git, make_stale, write};

    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "a.txt", b"one\n");
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "--quiet", "-m", "init"]);
        tmp
    }

    #[test]
    fn git_dirs_are_the_same_outside_a_linked_worktree() {
        let tmp = repo();
        let review = Review::new(tmp.path());

        let dirs = review.git_dirs().unwrap();

        let git_dir = tmp.path().canonicalize().unwrap().join(".git");
        assert_eq!(dirs.git_dir.canonicalize().unwrap(), git_dir);
        assert_eq!(dirs.common_dir.canonicalize().unwrap(), git_dir);
    }

    #[test]
    fn a_linked_worktree_has_its_own_git_dir() {
        let tmp = repo();
        let linked = tmp.path().join("linked");
        git(
            tmp.path(),
            &["worktree", "add", "--quiet", "-b", "other", "linked"],
        );

        let dirs = Review::new(&linked).git_dirs().unwrap();

        let common = tmp.path().canonicalize().unwrap().join(".git");
        assert_eq!(dirs.common_dir.canonicalize().unwrap(), common);
        assert_eq!(
            dirs.git_dir.canonicalize().unwrap(),
            common.join("worktrees/linked")
        );
    }

    #[test]
    fn snapshot_changes_with_each_kind_of_change() {
        let tmp = repo();
        let review = Review::new(tmp.path());
        let clean = review.snapshot().unwrap();

        write(tmp.path(), "a.txt", b"two\n");
        let edited = review.snapshot().unwrap();
        assert_ne!(edited.files, clean.files);
        assert_eq!(edited.index, clean.index);
        assert_eq!(edited.refs, clean.refs);

        write(tmp.path(), "a.txt", b"three, longer\n");
        let edited_again = review.snapshot().unwrap();
        assert_ne!(edited_again.files, edited.files);

        git(tmp.path(), &["add", "a.txt"]);
        let staged = review.snapshot().unwrap();
        assert_ne!(staged.index, edited_again.index);
        assert_eq!(staged.refs, clean.refs);

        git(tmp.path(), &["commit", "--quiet", "-m", "second"]);
        let committed = review.snapshot().unwrap();
        assert_ne!(committed.refs, staged.refs);

        git(tmp.path(), &["branch", "other"]);
        assert_ne!(review.snapshot().unwrap().refs, committed.refs);
    }

    #[test]
    fn snapshot_sees_untracked_files() {
        let tmp = repo();
        let review = Review::new(tmp.path());
        let clean = review.snapshot().unwrap();

        write(tmp.path(), "new/b.txt", b"b\n");
        let added = review.snapshot().unwrap();
        assert_ne!(added.files, clean.files);

        write(tmp.path(), "new/b.txt", b"b, longer\n");
        assert_ne!(review.snapshot().unwrap().files, added.files);
    }

    #[test]
    fn snapshot_works_on_an_unborn_branch() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "a.txt", b"one\n");

        let snapshot = Review::new(tmp.path()).snapshot().unwrap();

        assert!(!snapshot.files.is_empty());
    }

    #[test]
    fn snapshot_never_writes_the_index() {
        let tmp = repo();
        make_stale(tmp.path(), "a.txt");
        let index = tmp.path().join(".git/index");
        let before = std::fs::metadata(&index).unwrap().modified().unwrap();

        Review::new(tmp.path()).snapshot().unwrap();

        assert_eq!(
            std::fs::metadata(&index).unwrap().modified().unwrap(),
            before
        );
    }

    #[test]
    fn parses_status_entries() {
        let modified = b"1 .M N... 100644 100644 100644 aaa aaa src/a b.txt";
        assert_eq!(
            parse_status(modified),
            Some((None, Some(&b"src/a b.txt"[..])))
        );
        let staged = b"1 M. N... 100644 100644 100644 aaa bbb a.txt";
        assert_eq!(parse_status(staged), Some((Some(&staged[..]), None)));
        assert_eq!(
            parse_status(b"? new.txt"),
            Some((None, Some(&b"new.txt"[..])))
        );
        assert_eq!(parse_status(b""), None);
    }
}
