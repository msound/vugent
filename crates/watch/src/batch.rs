//! When a batch of file system events is complete: once the folder has been quiet for a while, but no later than a
//! fixed time after the batch's first event, so that an agent that writes without pause still shows progress.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct Batcher {
    quiet: Duration,
    max: Duration,
    /// When the batch's first and last events arrived; `None` while the batch is empty.
    times: Option<(Instant, Instant)>,
}

impl Batcher {
    pub(crate) fn new(quiet: Duration, max: Duration) -> Self {
        Self {
            quiet,
            max,
            times: None,
        }
    }

    /// Adds an event that arrived at `now`.
    pub(crate) fn push(&mut self, now: Instant) {
        let first = self.times.map_or(now, |(first, _)| first);
        self.times = Some((first, now));
    }

    /// When the batch is complete, or `None` while it is empty.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.times
            .map(|(first, last)| (last + self.quiet).min(first + self.max))
    }

    /// Starts the next batch.
    pub(crate) fn reset(&mut self) {
        self.times = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUIET: Duration = Duration::from_millis(150);
    const MAX: Duration = Duration::from_secs(1);

    #[test]
    fn an_empty_batch_has_no_deadline() {
        assert_eq!(Batcher::new(QUIET, MAX).deadline(), None);
    }

    #[test]
    fn waits_until_quiet() {
        let start = Instant::now();
        let mut batcher = Batcher::new(QUIET, MAX);

        batcher.push(start);
        assert_eq!(batcher.deadline(), Some(start + QUIET));

        batcher.push(start + Duration::from_millis(100));
        assert_eq!(batcher.deadline(), Some(start + Duration::from_millis(250)));
    }

    #[test]
    fn never_waits_longer_than_the_maximum() {
        let start = Instant::now();
        let mut batcher = Batcher::new(QUIET, MAX);

        for millis in (0..=950).step_by(50) {
            batcher.push(start + Duration::from_millis(millis));
        }

        assert_eq!(batcher.deadline(), Some(start + MAX));
    }

    #[test]
    fn reset_starts_a_new_batch() {
        let start = Instant::now();
        let mut batcher = Batcher::new(QUIET, MAX);
        batcher.push(start);

        batcher.reset();
        assert_eq!(batcher.deadline(), None);

        let later = start + Duration::from_secs(5);
        batcher.push(later);
        assert_eq!(batcher.deadline(), Some(later + QUIET));
    }
}
