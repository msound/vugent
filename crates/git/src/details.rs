//! What the detail page of ADR 0007 shows about a commit: its whole message, trailers, author and committer.

use serde::Serialize;

use crate::review::Review;
use crate::{Error, run, run_with};

/// Everything the detail page shows about one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails {
    pub sha: String,
    pub short_sha: String,
    /// The first paragraph of the message, on one line.
    pub subject: String,
    /// The rest of the message, without the trailers.
    pub body: String,
    /// The trailers at the end of the message, such as `Closes: #25`, in order.
    pub trailers: Vec<Trailer>,
    pub author: Signature,
    pub committer: Signature,
}

/// A trailer at the end of a commit message, such as `Co-Authored-By: Name <email>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trailer {
    pub key: String,
    pub value: String,
}

/// Who wrote or committed a commit, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub name: String,
    pub email: String,
    /// In strict ISO 8601, with the offset from UTC that the commit was made in.
    pub date: String,
}

/// The format of `git show`: fields separated by NUL, the body last because it spans lines.
const FORMAT: &str = "--format=%H%x00%h%x00%s%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%(trailers:only,unfold)%x00%b";

impl Review {
    /// Reads the commit `sha`. Fails with [`Error::NotFound`] when there is no such commit.
    pub fn commit_details(&self, sha: &str) -> Result<CommitDetails, Error> {
        // A leading dash would be read as an option.
        if sha.starts_with('-') {
            return Err(Error::NotFound(sha.to_owned()));
        }
        let Some(sha) = self.commit(sha)? else {
            return Err(Error::NotFound(sha.to_owned()));
        };
        let out = run(
            self.dir(),
            &["show", "-s", "--no-show-signature", FORMAT, &sha, "--"],
        )?;
        let out = String::from_utf8_lossy(&out);
        let mut fields = out.splitn(11, '\0');
        let mut next = || fields.next().unwrap_or_default().to_owned();
        let (sha, short_sha, subject) = (next(), next(), next());
        let author = Signature {
            name: next(),
            email: next(),
            date: next(),
        };
        let committer = Signature {
            name: next(),
            email: next(),
            date: next(),
        };
        let trailers = parse_trailers(&next());
        let body = strip_trailers(&next(), !trailers.is_empty());
        Ok(CommitDetails {
            sha,
            short_sha,
            subject,
            body,
            trailers,
            author,
            committer,
        })
    }

    /// The repository's web address, such as `https://github.com/owner/repo`, when `origin` is on GitHub.
    pub fn github(&self) -> Result<Option<String>, Error> {
        // ls-remote --get-url applies url.<base>.insteadOf, and prints the name itself when there is no such remote.
        let out = run_with(self.dir(), &["ls-remote", "--get-url", "origin"], &[0])?;
        Ok(github_url(String::from_utf8_lossy(&out).trim()))
    }
}

/// Reads the `Key: value` lines that `%(trailers:only,unfold)` prints.
fn parse_trailers(text: &str) -> Vec<Trailer> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some(Trailer {
                key: key.trim().to_owned(),
                value: value.trim().to_owned(),
            })
        })
        .collect()
}

/// The body without its trailers, which git only finds in the last paragraph.
fn strip_trailers(body: &str, has_trailers: bool) -> String {
    let body = body.trim_end();
    if !has_trailers {
        return body.to_owned();
    }
    match body.rfind("\n\n") {
        Some(end) => body[..end].trim_end().to_owned(),
        // The trailers are the whole body.
        None => String::new(),
    }
}

/// The web address of a GitHub repository from a remote URL in any of the forms git accepts.
fn github_url(remote: &str) -> Option<String> {
    let path = ["https://", "http://", "ssh://", "git://", "git+ssh://"]
        .iter()
        .find_map(|scheme| remote.strip_prefix(scheme))
        .and_then(|rest| {
            // Drop a user, as in https://user@github.com/ or ssh://git@github.com/.
            let rest = rest.split_once('@').map_or(rest, |(_, host)| host);
            let (host, path) = rest.split_once('/')?;
            // ssh://git@github.com:22/owner/repo
            let host = host.split_once(':').map_or(host, |(host, _)| host);
            (host == "github.com" || host == "www.github.com").then_some(path)
        })
        .or_else(|| {
            // scp-like syntax: git@github.com:owner/repo.git
            let (host, path) = remote.split_once(':')?;
            let host = host.split_once('@').map_or(host, |(_, host)| host);
            (host == "github.com").then_some(path)
        })?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    };
    (valid(owner) && valid(repo)).then(|| format!("https://github.com/{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{git, write};

    fn repo_with(message: &str) -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "a.txt", b"a\n");
        git(tmp.path(), &["add", "a.txt"]);
        git(tmp.path(), &["commit", "--quiet", "-m", message]);
        let sha = git(tmp.path(), &["rev-parse", "HEAD"]).trim().to_owned();
        (tmp, sha)
    }

    #[test]
    fn reads_the_message_and_trailers() {
        let (tmp, sha) = repo_with(
            "feat: reticulate splines\n\nThe splines were flat.\n\nNow they are not.\n\nRefs: #1\nCo-Authored-By: A <a@example.com>",
        );

        let details = Review::new(tmp.path()).commit_details(&sha).unwrap();

        assert_eq!(details.sha, sha);
        assert!(sha.starts_with(&details.short_sha));
        assert_eq!(details.subject, "feat: reticulate splines");
        assert_eq!(details.body, "The splines were flat.\n\nNow they are not.");
        assert_eq!(
            details.trailers,
            [
                Trailer {
                    key: "Refs".to_owned(),
                    value: "#1".to_owned()
                },
                Trailer {
                    key: "Co-Authored-By".to_owned(),
                    value: "A <a@example.com>".to_owned()
                },
            ]
        );
        assert_eq!(details.author.name, "Test");
        assert_eq!(details.author.email, "test@example.com");
        assert!(details.author.date.contains('T'), "{}", details.author.date);
        assert_eq!(details.committer, details.author);
    }

    #[test]
    fn a_message_without_trailers_keeps_its_last_paragraph() {
        let (tmp, sha) = repo_with("fix: things\n\nFirst.\n\nThen, really");

        let details = Review::new(tmp.path()).commit_details(&sha).unwrap();

        assert!(details.trailers.is_empty());
        assert!(details.body.ends_with("really"), "{:?}", details.body);
    }

    #[test]
    fn a_subject_with_only_trailers_has_an_empty_body() {
        let (tmp, sha) = repo_with("chore: bump\n\nCloses: #3");

        let details = Review::new(tmp.path()).commit_details(&sha).unwrap();

        assert_eq!(details.body, "");
        assert_eq!(details.trailers[0].value, "#3");
    }

    #[test]
    fn an_unknown_commit_is_not_found() {
        let (tmp, _) = repo_with("first");
        let review = Review::new(tmp.path());

        for sha in ["0".repeat(40), "--all".to_owned()] {
            let error = review.commit_details(&sha).unwrap_err();
            assert!(matches!(error, Error::NotFound(_)), "{error:?}");
        }
    }

    #[test]
    fn finds_github_from_origin() {
        let (tmp, _) = repo_with("first");
        let review = Review::new(tmp.path());
        assert_eq!(review.github().unwrap(), None);

        git(
            tmp.path(),
            &["remote", "add", "origin", "git@github.com:owner/repo.git"],
        );

        assert_eq!(
            review.github().unwrap().as_deref(),
            Some("https://github.com/owner/repo")
        );
    }

    #[test]
    fn reads_github_urls_in_every_form() {
        for remote in [
            "https://github.com/owner/repo",
            "https://github.com/owner/repo.git",
            "https://user@github.com/owner/repo.git/",
            "ssh://git@github.com/owner/repo.git",
            "ssh://git@github.com:22/owner/repo",
            "git@github.com:owner/repo.git",
            "git://github.com/owner/repo.git",
        ] {
            assert_eq!(
                github_url(remote).as_deref(),
                Some("https://github.com/owner/repo"),
                "{remote}"
            );
        }
        for remote in [
            "origin",
            "https://gitlab.com/owner/repo",
            "git@gitlab.com:owner/repo.git",
            "https://github.com/owner",
            "https://github.com/owner/repo/extra",
            "/srv/git/repo.git",
            "https://github.com/ow<ner/repo",
        ] {
            assert_eq!(github_url(remote), None, "{remote}");
        }
    }
}
