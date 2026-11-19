//! Polling git when the reviewed folder cannot be watched: comparing a [`vugent_git::Snapshot`] with the previous one,
//! at most every few seconds and never keeping a CPU core more than about a tenth busy.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use vugent_git::{Review, Snapshot};

use crate::{Config, Event, Message};

/// Polls until stopped, handing what changed to `sink`.
pub(crate) fn run(
    review: &Review,
    config: &Config,
    receiver: &mpsc::Receiver<Message>,
    sink: &mut impl FnMut(Event),
) {
    let (mut last, mut took) = timed(review);
    let mut next = Instant::now() + interval(config.poll_interval, took);
    loop {
        match receiver.recv_timeout(next.saturating_duration_since(Instant::now())) {
            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            // Left over from a watcher that failed.
            Ok(Message::Notify(_)) => continue,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let snapshot;
        (snapshot, took) = timed(review);
        next = Instant::now() + interval(config.poll_interval, took);
        // A poll that fails, such as while git holds a lock, is compared again on the next one.
        let (Some(old), Some(new)) = (&last, &snapshot) else {
            last = last.or(snapshot);
            continue;
        };
        for event in changes(old, new) {
            sink(event);
        }
        last = snapshot;
    }
}

fn timed(review: &Review) -> (Option<Snapshot>, Duration) {
    let start = Instant::now();
    let snapshot = review.snapshot().ok();
    (snapshot, start.elapsed())
}

/// How long to wait after a poll that took `took`: `shortest`, or 10 times as long as the poll if that is longer.
fn interval(shortest: Duration, took: Duration) -> Duration {
    shortest.max(took * 10)
}

/// The events that tell what differs from `old` to `new`. A poll does not know which files changed.
fn changes(old: &Snapshot, new: &Snapshot) -> Vec<Event> {
    let mut events = Vec::new();
    if old.files != new.files {
        events.push(Event::Files { paths: None });
    }
    if old.index != new.index {
        events.push(Event::Index);
    }
    if old.refs != new.refs {
        events.push(Event::Refs);
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::start_with;
    use crate::testing::{expect, expect_none, git, repo, write};

    #[test]
    fn waits_ten_times_as_long_as_a_slow_poll() {
        let five = Duration::from_secs(5);

        assert_eq!(interval(five, Duration::from_millis(30)), five);
        assert_eq!(
            interval(five, Duration::from_secs(2)),
            Duration::from_secs(20)
        );
    }

    #[test]
    fn says_what_changed() {
        let old = Snapshot {
            files: b"a".to_vec(),
            index: b"i".to_vec(),
            refs: b"r".to_vec(),
        };

        assert!(changes(&old, &old.clone()).is_empty());
        let new = Snapshot {
            files: b"b".to_vec(),
            refs: b"s".to_vec(),
            ..old.clone()
        };
        assert_eq!(
            changes(&old, &new),
            [Event::Files { paths: None }, Event::Refs]
        );
    }

    #[test]
    fn polls_git() {
        let (_tmp, dir) = repo();
        let config = Config {
            poll_interval: Duration::from_millis(100),
            ..Config::new(true)
        };
        let (sender, events) = mpsc::channel();
        let _watch = start_with(Review::new(&dir), config, move |event| {
            let _ = sender.send(event);
        });
        std::thread::sleep(Duration::from_millis(300));

        write(&dir, "target/out.txt", "rebuilt\n");
        expect_none(&events, |_| true);
        write(&dir, "src/a.txt", "two\n");
        expect(&events, |event| *event == Event::Files { paths: None });
        git(&dir, &["add", "src/a.txt"]);
        expect(&events, |event| *event == Event::Index);
        git(&dir, &["commit", "--quiet", "-m", "two"]);

        expect(&events, |event| *event == Event::Refs);
    }
}
