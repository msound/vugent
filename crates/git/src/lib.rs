//! Runs the `git` CLI and parses its output.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// An error from running git.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The `git` executable could not be started, usually because it is not on `PATH`.
    #[error("could not run git (is it installed and on PATH?): {0}")]
    Spawn(#[source] io::Error),
    /// git ran but exited with a failure.
    #[error("git {args} failed: {stderr}")]
    Failed { args: String, stderr: String },
}

/// Returns the root of the working tree that contains `path`.
pub fn repo_root(path: &Path) -> Result<PathBuf, Error> {
    let out = run(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim_end_matches(['\n', '\r'])))
}

/// Runs `git -C <dir> <args>` and returns its stdout.
fn run(dir: &Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(Error::Spawn)?;
    if !output.status.success() {
        return Err(Error::Failed {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_init(dir: &Path) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["init", "--quiet"])
            .status()
            .expect("git init");
        assert!(status.success());
    }

    #[test]
    fn repo_root_finds_root_from_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        git_init(tmp.path());
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
