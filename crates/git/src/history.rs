//! The reviews the sidebar of ADR 0005 offers: the branch, uncommitted and staged changes, and recent commits.

use serde::Serialize;

use crate::review::Review;
use crate::{Error, run, run_code, run_with};

/// How many commits are listed when there is no branch to limit them.
const FALLBACK_COMMITS: &str = "50";

/// What can be reviewed in the reviewed folder right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    /// The current branch's name, or the short SHA of a detached `HEAD`. `None` when there is no branch review: on
    /// the main branch itself, when no main branch is found, or when no commit of the branch touches the folder.
    pub branch: Option<String>,
    /// Something in the folder differs from `HEAD`, or is untracked.
    pub uncommitted: bool,
    /// Something in the folder is staged.
    pub staged: bool,
    /// Newest first, following first parents, only those that touch the folder: the branch's commits, or the most
    /// recent ones when there is no branch.
    pub commits: Vec<Commit>,
}

/// One commit of the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub sha: String,
    pub short_sha: String,
    /// The first line of the commit message.
    pub subject: String,
}

impl Review {
    /// Lists what can be reviewed: see [`History`].
    pub fn history(&self) -> Result<History, Error> {
        let head = self.commit("HEAD")?;
        let base = match &head {
            Some(head) => head.clone(),
            None => self.empty_tree()?,
        };
        let uncommitted = self.differs(&[&base])? || self.has_untracked()?;
        let staged = self.differs(&["--cached", &base])?;
        let mut history = History {
            branch: None,
            uncommitted,
            staged,
            commits: Vec::new(),
        };
        let Some(head) = head else {
            return Ok(history);
        };

        if let Some(branch_base) = self.branch_base()? {
            history.commits = self.log(&[&format!("{branch_base}..{head}")])?;
            if !history.commits.is_empty() {
                history.branch = Some(self.branch_name()?);
                return Ok(history);
            }
        }
        history.commits = self.log(&["-n", FALLBACK_COMMITS, &head])?;
        Ok(history)
    }

    /// Whether `git diff <range>` finds a change in the folder.
    fn differs(&self, range: &[&str]) -> Result<bool, Error> {
        let mut args = vec!["diff"];
        args.extend(range);
        args.extend(["--quiet", "--no-ext-diff", "--no-textconv", "--", "."]);
        // --quiet exits with 1 when there are changes.
        let (code, _) = run_code(self.dir(), &args, &[0, 1])?;
        Ok(code == 1)
    }

    fn has_untracked(&self) -> Result<bool, Error> {
        let out = run(
            self.dir(),
            &["ls-files", "--others", "--exclude-standard", "--", "."],
        )?;
        Ok(!out.is_empty())
    }

    /// The commits `git log` lists for `revs`, following first parents, that touch the folder.
    fn log(&self, revs: &[&str]) -> Result<Vec<Commit>, Error> {
        let mut args = vec![
            "log",
            "--first-parent",
            "--no-show-signature",
            "--format=%H%x00%h%x00%s",
        ];
        args.extend(revs);
        args.extend(["--", "."]);
        let out = run(self.dir(), &args)?;
        Ok(String::from_utf8_lossy(&out)
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(3, '\0');
                Some(Commit {
                    sha: fields.next()?.to_owned(),
                    short_sha: fields.next()?.to_owned(),
                    subject: fields.next().unwrap_or_default().to_owned(),
                })
            })
            .collect())
    }

    /// The current branch's short name, or the short SHA of a detached `HEAD`.
    fn branch_name(&self) -> Result<String, Error> {
        // symbolic-ref --quiet exits with 1 on a detached HEAD.
        let name = run_with(
            self.dir(),
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            &[0, 1],
        )?;
        let name = String::from_utf8_lossy(&name).trim().to_owned();
        if !name.is_empty() {
            return Ok(name);
        }
        let short = run(self.dir(), &["rev-parse", "--short", "HEAD"])?;
        Ok(String::from_utf8_lossy(&short).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{git, write};

    fn commit(dir: &std::path::Path, path: &str, content: &str, message: &str) -> String {
        write(dir, path, content.as_bytes());
        git(dir, &["add", path]);
        git(dir, &["commit", "--quiet", "-m", message]);
        git(dir, &["rev-parse", "HEAD"]).trim().to_owned()
    }

    fn subjects(history: &History) -> Vec<&str> {
        history
            .commits
            .iter()
            .map(|commit| commit.subject.as_str())
            .collect()
    }

    /// A repository with two commits on `main`.
    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        commit(tmp.path(), "a.txt", "a\n", "first");
        commit(tmp.path(), "src/b.txt", "b\n", "second");
        tmp
    }

    #[test]
    fn on_the_main_branch_lists_recent_commits_without_a_branch() {
        let tmp = repo();

        let history = Review::new(tmp.path()).history().unwrap();

        assert_eq!(history.branch, None);
        assert_eq!(subjects(&history), ["second", "first"]);
        let head = &history.commits[0];
        assert_eq!(head.sha, git(tmp.path(), &["rev-parse", "HEAD"]).trim());
        assert!(head.sha.starts_with(&head.short_sha));
        assert!(!history.uncommitted && !history.staged);
    }

    #[test]
    fn on_a_branch_lists_only_its_commits() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "-b", "feature"]);
        commit(tmp.path(), "a.txt", "a2\n", "third");
        commit(tmp.path(), "src/b.txt", "b2\n", "fourth");

        let history = Review::new(tmp.path()).history().unwrap();

        assert_eq!(history.branch.as_deref(), Some("feature"));
        assert_eq!(subjects(&history), ["fourth", "third"]);
    }

    #[test]
    fn lists_only_commits_that_touch_the_folder() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "-b", "feature"]);
        commit(tmp.path(), "a.txt", "a2\n", "outside");
        commit(tmp.path(), "src/c.txt", "c\n", "inside");

        let review = Review::new(tmp.path().join("src"));
        let history = review.history().unwrap();

        assert_eq!(history.branch.as_deref(), Some("feature"));
        assert_eq!(subjects(&history), ["inside"]);
    }

    #[test]
    fn a_branch_without_commits_in_the_folder_falls_back_to_recent_commits() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "-b", "feature"]);
        commit(tmp.path(), "a.txt", "a2\n", "outside");

        let history = Review::new(tmp.path().join("src")).history().unwrap();

        assert_eq!(history.branch, None);
        assert_eq!(subjects(&history), ["second"]);
    }

    #[test]
    fn follows_first_parents_only() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "-b", "feature"]);
        commit(tmp.path(), "f.txt", "f\n", "on feature");
        git(tmp.path(), &["checkout", "--quiet", "-b", "side", "main"]);
        commit(tmp.path(), "s.txt", "s\n", "on side");
        git(tmp.path(), &["checkout", "--quiet", "feature"]);
        git(
            tmp.path(),
            &["merge", "--quiet", "--no-ff", "-m", "merge side", "side"],
        );

        let history = Review::new(tmp.path()).history().unwrap();

        assert_eq!(subjects(&history), ["merge side", "on feature"]);
    }

    #[test]
    fn names_a_detached_head_by_its_short_sha() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "-b", "feature"]);
        let sha = commit(tmp.path(), "a.txt", "a2\n", "third");
        git(tmp.path(), &["checkout", "--quiet", "--detach"]);

        let history = Review::new(tmp.path()).history().unwrap();

        let name = history.branch.unwrap();
        assert!(sha.starts_with(&name), "{name} is not a prefix of {sha}");
    }

    #[test]
    fn the_main_branch_can_be_given() {
        let tmp = repo();
        git(tmp.path(), &["branch", "trunk", "HEAD~1"]);

        let review = Review::new(tmp.path()).with_main("trunk");

        assert_eq!(review.main_branch().unwrap().as_deref(), Some("trunk"));
        let history = review.history().unwrap();
        assert_eq!(history.branch.as_deref(), Some("main"));
        assert_eq!(subjects(&history), ["second"]);
    }

    #[test]
    fn a_missing_main_branch_is_none() {
        let tmp = repo();

        let review = Review::new(tmp.path()).with_main("nope");

        assert_eq!(review.main_branch().unwrap(), None);
        assert_eq!(review.history().unwrap().branch, None);
        let dashed = Review::new(tmp.path()).with_main("--all");
        assert_eq!(dashed.main_branch().unwrap(), None);
    }

    #[test]
    fn tells_uncommitted_and_staged_changes_apart() {
        let tmp = repo();
        let dir = tmp.path();
        let review = Review::new(dir);

        write(dir, "new.txt", b"n\n");
        let untracked = review.history().unwrap();
        write(dir, "src/b.txt", b"changed\n");
        git(dir, &["add", "src/b.txt"]);
        std::fs::remove_file(dir.join("new.txt")).unwrap();
        let staged = review.history().unwrap();

        assert!(untracked.uncommitted && !untracked.staged);
        assert!(staged.uncommitted && staged.staged);
    }

    #[test]
    fn changes_outside_the_folder_do_not_count() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"changed\n");
        git(tmp.path(), &["add", "a.txt"]);
        write(tmp.path(), "loose.txt", b"l\n");

        let history = Review::new(tmp.path().join("src")).history().unwrap();

        assert!(!history.uncommitted && !history.staged);
    }

    #[test]
    fn a_repo_without_commits_has_no_commits() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "a.txt", b"a\n");
        git(tmp.path(), &["add", "a.txt"]);

        let history = Review::new(tmp.path()).history().unwrap();

        assert_eq!(history.branch, None);
        assert!(history.commits.is_empty());
        assert!(history.uncommitted && history.staged);
    }
}
