//! Runs the `git` CLI and parses its output.

mod details;
mod diff;
mod history;
mod review;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use details::{CommitDetails, Signature, Trailer};
pub use diff::{FileDiff, Hunk, Line, LineKind};
pub use history::{Base, Commit, History};
pub use review::{FileEntry, Rev, Review, Status};

/// An error from running git.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The `git` executable could not be started, usually because it is not on `PATH`.
    #[error("could not run git (is it installed and on PATH?)")]
    Spawn(#[source] io::Error),
    /// git ran but exited with a failure.
    #[error("git {args} failed: {stderr}")]
    Failed { args: String, stderr: String },
    /// A file in the working tree could not be read.
    #[error("cannot read {path}")]
    Read {
        path: String,
        #[source]
        source: io::Error,
    },
    /// The requested path is not part of the review.
    #[error("{0} is not part of the review")]
    NotFound(String),
}

/// Returns the output of `git --version`, such as `git version 2.47.1`. Use it to check that git is installed.
pub fn version() -> Result<String, Error> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .map_err(Error::Spawn)?;
    if !output.status.success() {
        return Err(Error::Failed {
            args: "--version".to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Returns the root of the working tree that contains `path`.
pub fn repo_root(path: &Path) -> Result<PathBuf, Error> {
    let out = run(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(
        String::from_utf8_lossy(&out).trim_end_matches(['\n', '\r']),
    ))
}

/// Runs `git -C <dir> <args>` and returns its stdout, failing unless git exits with one of `ok_codes`.
fn run_with(dir: &Path, args: &[&str], ok_codes: &[i32]) -> Result<Vec<u8>, Error> {
    run_input(dir, args, None, ok_codes)
}

/// Runs `git -C <dir> <args>` with `input` on its stdin and returns its stdout.
fn run_stdin(dir: &Path, args: &[&str], input: &[u8]) -> Result<Vec<u8>, Error> {
    run_input(dir, args, Some(input), &[0])
}

/// Runs `git -C <dir> <args>`, feeding it `input` when given, and returns its stdout, failing unless git exits with
/// one of `ok_codes`.
fn run_input(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    ok_codes: &[i32],
) -> Result<Vec<u8>, Error> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        // vugent only reads: stop git from taking optional locks, such as for refreshing the index. Porcelain `git
        // diff` refreshes the index regardless, so worktree comparisons use plumbing instead.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::Spawn)?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        // Write from another thread, so that git filling its stdout cannot block us both.
        let input = input.to_vec();
        std::thread::spawn(move || stdin.write_all(&input));
    }
    let output = child.wait_with_output().map_err(Error::Spawn)?;
    match output.status.code() {
        Some(code) if ok_codes.contains(&code) => Ok(output.stdout),
        _ => Err(Error::Failed {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        }),
    }
}

/// Runs `git -C <dir> <args>` and returns its stdout.
fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>, Error> {
    run_with(dir, args, &[0])
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;
    use std::process::Command;

    /// Runs git in `dir` for test setup, isolated from the user's and the system's git configuration.
    pub fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Writes `content` to `path` under `dir`, creating parent folders.
    pub fn write(dir: &Path, path: &str, content: &[u8]) {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// Gives the file at `path` under `dir` a modification time from long ago, so that the index's stat data no
    /// longer matches it although its content does. That tempts git into refreshing the index.
    pub fn make_stale(dir: &Path, path: &str) {
        let file = std::fs::File::options()
            .write(true)
            .open(dir.join(path))
            .unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testing::git;
    use super::*;

    #[test]
    fn version_reports_git_version() {
        let version = version().unwrap();

        assert!(version.starts_with("git version "), "got {version:?}");
    }

    #[test]
    fn repo_root_finds_root_from_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        let sub = tmp.path().join("a/b");
        std::fs::create_dir_all(&sub).unwrap();

        let root = repo_root(&sub).unwrap();

        assert_eq!(
            root.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn repo_root_fails_outside_a_repo() {
        let tmp = tempfile::tempdir().unwrap();

        let err = repo_root(tmp.path()).unwrap_err();

        assert!(matches!(err, Error::Failed { .. }), "got {err:?}");
    }
}
