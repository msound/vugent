//! Checks that the built frontend is present, and works out the version shown in the UI.
//!
//! The frontend is optional for development, but release builds set `VUGENT_REQUIRE_FRONTEND=1` so that a binary is
//! never shipped without the UI.
//!
//! The version comes from `git describe --tags --dirty`: `v0.2.0` at a tag, `v0.2.0+git.3.g90ba43f` three commits
//! past it, and `v0.2.0+git.3.g90ba43f.dirty` with uncommitted changes. It is `v` followed by the crate's version when
//! git or the tags are unavailable, as when building from a source tarball. It is passed to the code as
//! `VUGENT_VERSION`. The `dirty` mark is best-effort: this script only runs again when the crate's sources, the
//! built frontend or a git ref change, so other edits are not noticed until then.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=frontend");
    println!("cargo::rerun-if-env-changed=VUGENT_REQUIRE_FRONTEND");

    version();
    check_frontend();
}

fn version() {
    // The crate is rebuilt when its sources change anyway, so the script may as well rerun to update the `dirty` mark.
    println!("cargo::rerun-if-changed=src");
    let version = git(&["describe", "--tags", "--dirty"])
        .map(|described| mark_dev_build(&described))
        .unwrap_or_else(|| format!("v{}", std::env::var("CARGO_PKG_VERSION").unwrap()));
    println!("cargo::rustc-env=VUGENT_VERSION={version}");

    // Run again when a commit is made, a branch is checked out or a tag is added, so that the version stays current.
    if let Some(git_dir) = git(&["rev-parse", "--git-dir"]).map(PathBuf::from) {
        let mut paths = vec![git_dir.join("HEAD")];
        // Refs live in the common git directory, which differs from the git directory in a linked worktree.
        if let Some(common_dir) = git(&["rev-parse", "--git-common-dir"]).map(PathBuf::from) {
            paths.push(common_dir.join("refs"));
            paths.push(common_dir.join("packed-refs"));
        }
        for path in paths {
            println!("cargo::rerun-if-changed={}", path.display());
        }
    }
}

/// Turns `git describe`'s `v0.2.0-3-g90ba43f[-dirty]`, three commits past the tag, into
/// `v0.2.0+git.3.g90ba43f[.dirty]`, semver build metadata that marks a build from git. A bare tag is left alone.
fn mark_dev_build(described: &str) -> String {
    let (rest, dirty) = match described.strip_suffix("-dirty") {
        Some(rest) => (rest, ".dirty"),
        None => (described, ""),
    };
    // The tag itself may hold dashes, so split from the right.
    let mut parts = rest.rsplitn(3, '-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(sha), Some(count), Some(tag))
            if sha.starts_with('g')
                && !count.is_empty()
                && count.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            format!("{tag}+git.{count}.{sha}{dirty}")
        }
        _ => described.to_owned(),
    }
}

/// Runs git in this crate's folder, returning its trimmed output, or `None` when git is missing or fails.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn check_frontend() {
    if Path::new("frontend/index.html").is_file() {
        return;
    }
    let required = std::env::var("VUGENT_REQUIRE_FRONTEND").is_ok_and(|value| value == "1");
    if required {
        panic!(
            "VUGENT_REQUIRE_FRONTEND=1 but the frontend is not built: run `npm ci && npm run build` in web/"
        );
    }
    println!(
        "cargo::warning=the frontend is not built, so the UI will be missing: run `npm install && npm run build` in web/"
    );
}
