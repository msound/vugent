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
const FORMAT: &str = "--format=%H%x00%h%x00%s%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%b";

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
        let mut fields = out.splitn(10, '\0');
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
        let (body, trailers) = split_trailers(&next());
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

/// Splits the trailers off the end of a commit's body. git only finds trailers in the last paragraph, but footers
/// are often written as several paragraphs, such as `Closes: #25`, a blank line and `Co-Authored-By: …`, so every
/// paragraph at the end made only of trailers counts. Lines that start with whitespace continue the trailer above.
fn split_trailers(body: &str) -> (String, Vec<Trailer>) {
    let mut paragraphs: Vec<Vec<&str>> = vec![Vec::new()];
    for line in body.lines() {
        if line.trim().is_empty() {
            paragraphs.push(Vec::new());
        } else if let Some(paragraph) = paragraphs.last_mut() {
            paragraph.push(line);
        }
    }
    paragraphs.retain(|paragraph| !paragraph.is_empty());

    let mut keep = paragraphs.len();
    while keep > 0 && is_trailer_block(&paragraphs[keep - 1]) {
        keep -= 1;
    }
    let mut trailers: Vec<Trailer> = Vec::new();
    for line in paragraphs[keep..].iter().flatten() {
        if line.starts_with([' ', '\t']) {
            if let Some(last) = trailers.last_mut() {
                last.value.push(' ');
                last.value.push_str(line.trim());
            }
        } else if let Some((key, value)) = trailer(line) {
            trailers.push(Trailer {
                key: key.to_owned(),
                value: value.to_owned(),
            });
        }
    }
    let text = paragraphs[..keep]
        .iter()
        .map(|paragraph| paragraph.join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n");
    (text, trailers)
}

/// Whether every line of `paragraph` is a trailer or continues one.
fn is_trailer_block(paragraph: &[&str]) -> bool {
    paragraph
        .first()
        .is_some_and(|first| trailer(first).is_some())
        && paragraph
            .iter()
            .all(|line| line.starts_with([' ', '\t']) || trailer(line).is_some())
}

/// The key and value of a trailer line such as `Co-Authored-By: Name <email>`.
fn trailer(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    let value = value.trim();
    let valid = !key.is_empty()
        && !value.is_empty()
        && key.starts_with(|c: char| c.is_ascii_alphanumeric())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    valid.then_some((key, value))
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
    fn footers_in_several_paragraphs_are_all_trailers() {
        let (tmp, sha) = repo_with(
            "feat: x\n\nBody.\n\nRefs: #25\n\nCo-Authored-By: A <a@example.com>\n  and friends\n",
        );

        let details = Review::new(tmp.path()).commit_details(&sha).unwrap();

        assert_eq!(details.body, "Body.");
        let trailers: Vec<_> = details
            .trailers
            .iter()
            .map(|trailer| (trailer.key.as_str(), trailer.value.as_str()))
            .collect();
        assert_eq!(
            trailers,
            [
                ("Refs", "#25"),
                ("Co-Authored-By", "A <a@example.com> and friends")
            ]
        );
    }

    #[test]
    fn a_paragraph_with_prose_is_not_a_trailer_block() {
        assert_eq!(
            split_trailers("Body.\n\nNote: this matters.\nIt really does.\n\nCloses: #1"),
            (
                "Body.\n\nNote: this matters.\nIt really does.".to_owned(),
                vec![Trailer {
                    key: "Closes".to_owned(),
                    value: "#1".to_owned()
                }]
            )
        );
        assert_eq!(
            split_trailers("See https://example.com: it explains.").1,
            []
        );
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
