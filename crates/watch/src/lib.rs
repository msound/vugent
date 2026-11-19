//! Tells when the reviewed folder, the index or the refs change, as decided in ADR 0008: by watching the file system
//! with `notify`, or by polling git when it cannot be watched.

mod batch;
mod gitignore;
mod poll;
mod watching;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use vugent_git::Review;

/// Something that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Files in the reviewed folder changed. `paths` are relative to the reviewed folder, with `/` separators, sorted;
    /// `None` when there were more than [`MAX_PATHS`] or which ones is not known, as when polling.
    Files { paths: Option<Vec<String>> },
    /// The index changed, such as after `git add`.
    Index,
    /// `HEAD` or a ref changed: a new commit, a branch switch, a rebase, a reset.
    Refs,
    /// Files can no longer be watched, so changes are polled from now on.
    Polling,
}

/// A `files` event lists at most this many paths.
pub const MAX_PATHS: usize = 1000;

/// How changes are detected and gathered.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    /// Poll from the start instead of watching.
    poll: bool,
    /// A batch of events is complete when the folder has been quiet this long…
    quiet: Duration,
    /// …or when its first event is this old.
    max_batch: Duration,
    /// The shortest time between polls; a poll that takes long waits 10 times as long.
    poll_interval: Duration,
    /// Watch every folder on its own, skipping gitignored ones, rather than the whole tree at once. Linux needs this,
    /// since every folder costs an inotify watch.
    per_folder: bool,
    /// The most watches added before acting as if the operating system ran out; only lowered by tests.
    max_watches: usize,
}

impl Config {
    fn new(poll: bool) -> Self {
        Self {
            poll,
            quiet: Duration::from_millis(150),
            max_batch: Duration::from_secs(1),
            poll_interval: Duration::from_secs(5),
            per_folder: cfg!(target_os = "linux"),
            max_watches: usize::MAX,
        }
    }
}

/// What the thread that watches or polls receives.
enum Message {
    Notify(notify::Result<notify::Event>),
    Stop,
}

/// Watches the reviewed folder until dropped.
pub struct Watch {
    sender: mpsc::Sender<Message>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        // The thread may have ended already; then there is nothing to stop.
        let _ = self.sender.send(Message::Stop);
    }
}

/// Starts watching the folder of `review` on a thread of its own, calling `sink` with what changes. With `poll`, or
/// when the folder cannot be watched, git is polled instead; in the latter case `sink` first gets
/// [`Event::Polling`]. Starting returns at once; changes made while the watches are being added may be missed.
pub fn start(review: Review, poll: bool, sink: impl FnMut(Event) + Send + 'static) -> Watch {
    start_with(review, Config::new(poll), sink)
}

fn start_with(
    review: Review,
    config: Config,
    mut sink: impl FnMut(Event) + Send + 'static,
) -> Watch {
    let (sender, receiver) = mpsc::channel();
    let notify_sender = sender.clone();
    std::thread::spawn(move || {
        if !config.poll {
            match watching::run(&review, &config, notify_sender, &receiver, &mut sink) {
                watching::Ended::Stopped => return,
                watching::Ended::Failed(error) => {
                    warn(&error, review.dir());
                    sink(Event::Polling);
                }
            }
        }
        poll::run(&review, &config, &receiver, &mut sink);
    });
    Watch { sender }
}

static WARNED: AtomicBool = AtomicBool::new(false);

/// Says once, on stderr, that vugent is polling instead of watching, and how to avoid it.
fn warn(error: &notify::Error, dir: &Path) {
    if WARNED.swap(true, Ordering::Relaxed) {
        return;
    }
    if matches!(error.kind, notify::ErrorKind::MaxFilesWatch) {
        eprintln!(
            "vugent ran out of file watches, so it checks for changes every few seconds instead.\n\
             To watch {dir} instead, raise the limit:\n\
             \n    sudo sysctl fs.inotify.max_user_watches=524288\n\n\
             and make it permanent with a file in /etc/sysctl.d/, such as\n\
             \n    echo fs.inotify.max_user_watches=524288 | sudo tee /etc/sysctl.d/90-inotify.conf\n\n\
             or review a subfolder, which only watches that folder.",
            dir = dir.display()
        );
    } else {
        eprintln!(
            "vugent cannot watch {} for changes ({error}), so it checks for changes every few seconds instead.",
            dir.display()
        );
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::Event;

    /// Runs git in `dir` for test setup, isolated from the user's and the system's git configuration.
    pub fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(["-c", "commit.gpgsign=false"])
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
    }

    pub fn write(dir: &Path, path: &str, content: &str) {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// A repository with a committed `src/a.txt` and an ignored `target/`, in its canonical location.
    pub fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        git(&dir, &["init", "--quiet"]);
        write(&dir, ".gitignore", "target/\n");
        write(&dir, "src/a.txt", "one\n");
        write(&dir, "target/out.txt", "build\n");
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "--quiet", "-m", "init"]);
        (tmp, dir)
    }

    /// Waits for an event that `wanted` accepts, failing after a few seconds. Other events are skipped.
    pub fn expect(events: &mpsc::Receiver<Event>, wanted: impl Fn(&Event) -> bool) -> Event {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match events.recv_timeout(left) {
                Ok(event) if wanted(&event) => return event,
                Ok(event) => seen.push(event),
                Err(_) => break,
            }
        }
        panic!("the expected event did not arrive; got {seen:?}");
    }

    /// Fails if an event that `unwanted` accepts arrives within half a second.
    pub fn expect_none(events: &mpsc::Receiver<Event>, unwanted: impl Fn(&Event) -> bool) {
        let deadline = Instant::now() + Duration::from_millis(500);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match events.recv_timeout(left) {
                Ok(event) => assert!(!unwanted(&event), "unexpected {event:?}"),
                Err(_) => return,
            }
        }
    }
}
