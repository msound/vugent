//! Watching the reviewed folder and the git directories with `notify`, and turning batches of its events into
//! [`Event`]s.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Instant;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use vugent_git::Review;

use crate::batch::Batcher;
use crate::gitignore::Ignores;
use crate::{Config, Event, MAX_PATHS, Message};

/// Why watching ended.
pub(crate) enum Ended {
    /// The [`crate::Watch`] was dropped.
    Stopped,
    /// The folder could not be watched, or could no longer be: poll instead.
    Failed(notify::Error),
}

/// Watches until stopped or until watching fails, handing each batch of changes to `sink`.
pub(crate) fn run(
    review: &Review,
    config: &Config,
    sender: mpsc::Sender<Message>,
    receiver: &mpsc::Receiver<Message>,
    sink: &mut impl FnMut(Event),
) -> Ended {
    let mut watching = match Watching::new(review, config, sender) {
        Ok(watching) => watching,
        Err(error) => return Ended::Failed(error),
    };
    let mut batcher = Batcher::new(config.quiet, config.max_batch);
    let mut batch = Batch::default();
    loop {
        let message = match batcher.deadline() {
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(deadline) => {
                receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }
        };
        match message {
            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return Ended::Stopped,
            Ok(Message::Notify(Ok(event))) => {
                if batch.add(event) {
                    batcher.push(Instant::now());
                }
            }
            Ok(Message::Notify(Err(error))) => {
                if matches!(error.kind, notify::ErrorKind::MaxFilesWatch) {
                    return Ended::Failed(error);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                batcher.reset();
                if let Err(error) = watching.handle(std::mem::take(&mut batch), sink) {
                    return Ended::Failed(error);
                }
            }
        }
    }
}

/// The paths touched by a batch of events.
#[derive(Debug, Default)]
struct Batch {
    paths: BTreeSet<PathBuf>,
    /// The operating system dropped events, so anything may have changed.
    rescan: bool,
}

impl Batch {
    /// Adds `event`, unless it only says that a file was read. Returns whether it was added.
    fn add(&mut self, event: notify::Event) -> bool {
        if event.need_rescan() {
            self.rescan = true;
            return true;
        }
        // Reading files, as git does whenever vugent asks it something, must not count as a change.
        if matches!(event.kind, EventKind::Access(_)) {
            return false;
        }
        self.paths.extend(event.paths);
        true
    }
}

/// What a path in an event is.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    /// A file or folder in the reviewed folder, relative to it.
    File(String),
    Index,
    Refs,
    /// Anything else: a lock file, the rest of `.git`, a gitignored path.
    Skip,
}

struct Watching<'a> {
    config: &'a Config,
    watcher: RecommendedWatcher,
    /// The reviewed folder.
    dir: PathBuf,
    /// The top of the working tree.
    root: PathBuf,
    /// Holds `HEAD` and `index`.
    git_dir: PathBuf,
    /// Holds `refs/` and `packed-refs`.
    common_dir: PathBuf,
    ignores: Ignores,
    /// The folders of the reviewed folder watched one by one, when [`Config::per_folder`] is set.
    watched: BTreeSet<PathBuf>,
    /// How many watches were added.
    count: usize,
}

impl<'a> Watching<'a> {
    fn new(
        review: &Review,
        config: &'a Config,
        sender: mpsc::Sender<Message>,
    ) -> notify::Result<Self> {
        let git_error = |error: vugent_git::Error| notify::Error::generic(&error.to_string());
        let dir = canonical(review.dir());
        let root = canonical(&vugent_git::repo_root(&dir).map_err(git_error)?);
        let dirs = review.git_dirs().map_err(git_error)?;
        let watcher = notify::recommended_watcher(move |result| {
            // The thread has ended when nobody receives; then the watcher is about to be dropped too.
            let _ = sender.send(Message::Notify(result));
        })?;
        let common_dir = canonical(&dirs.common_dir);
        let (ignores, folders) = Ignores::load(&root, &common_dir, &dir);
        let mut watching = Self {
            config,
            watcher,
            dir,
            root,
            git_dir: canonical(&dirs.git_dir),
            common_dir,
            ignores,
            watched: BTreeSet::new(),
            count: 0,
        };

        if config.per_folder {
            let dir = watching.dir.clone();
            watching.watch(&dir, RecursiveMode::NonRecursive)?;
            watching.watched.insert(dir.clone());
            for folder in folders.into_iter().filter(|folder| *folder != dir) {
                if watching.watch_folder(&folder)? {
                    watching.watched.insert(folder);
                }
            }
        } else {
            let dir = watching.dir.clone();
            watching.watch(&dir, RecursiveMode::Recursive)?;
        }

        // Events from the git directories are only taken from the few paths that count, in Kind. Watched whole, as
        // part of the reviewed folder, they need no watches of their own.
        let dir = watching.dir.clone();
        let covered = |path: &Path| !config.per_folder && path.starts_with(&dir);
        let git_dir = watching.git_dir.clone();
        let common_dir = watching.common_dir.clone();
        if !covered(&git_dir) {
            watching.watch(&git_dir, RecursiveMode::NonRecursive)?;
        }
        if !covered(&common_dir) {
            if common_dir != git_dir {
                watching.watch(&common_dir, RecursiveMode::NonRecursive)?;
            }
            watching.watch(&common_dir.join("refs"), RecursiveMode::Recursive)?;
        }
        Ok(watching)
    }

    /// Adds a watch, failing as if the operating system ran out of watches once [`Config::max_watches`] are used.
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        if self.count >= self.config.max_watches {
            return Err(notify::Error::new(notify::ErrorKind::MaxFilesWatch));
        }
        self.watcher.watch(path, mode)?;
        self.count += 1;
        Ok(())
    }

    /// Watches a folder inside the reviewed one, which may have been removed meanwhile. Returns whether it is
    /// watched; fails only when watches ran out.
    fn watch_folder(&mut self, folder: &Path) -> notify::Result<bool> {
        match self.watch(folder, RecursiveMode::NonRecursive) {
            Ok(()) => Ok(true),
            Err(error) if matches!(error.kind, notify::ErrorKind::MaxFilesWatch) => Err(error),
            Err(_) => Ok(false),
        }
    }

    fn classify(&self, path: &Path) -> Kind {
        if path
            .extension()
            .is_some_and(|extension| extension == "lock")
        {
            return Kind::Skip;
        }
        let parent = path.parent();
        let name = path.file_name().unwrap_or_default();
        if parent == Some(self.git_dir.as_path()) && name == "HEAD" {
            return Kind::Refs;
        }
        if parent == Some(self.git_dir.as_path()) && name == "index" {
            return Kind::Index;
        }
        if (parent == Some(self.common_dir.as_path()) && name == "packed-refs")
            || path.starts_with(self.common_dir.join("refs"))
        {
            return Kind::Refs;
        }
        let Ok(relative) = path.strip_prefix(&self.dir) else {
            return Kind::Skip;
        };
        if relative.as_os_str().is_empty() || self.ignores.is_ignored(path, is_folder(path)) {
            return Kind::Skip;
        }
        Kind::File(slashed(relative))
    }

    /// Turns a batch into events for `sink`, first following new and removed folders and changed ignore rules.
    fn handle(&mut self, batch: Batch, sink: &mut impl FnMut(Event)) -> notify::Result<()> {
        if batch.rescan {
            self.reload(&mut BTreeSet::new())?;
            sink(Event::Files { paths: None });
            sink(Event::Index);
            sink(Event::Refs);
            return Ok(());
        }

        if batch.paths.iter().any(|path| self.needs_reload(path)) {
            // Rules changed or folders came and went: what is ignored, and so what is watched, may differ now.
            let mut found = BTreeSet::new();
            self.reload(&mut found)?;
            return self.send(batch.paths.into_iter().chain(found), sink);
        }
        self.send(batch.paths.into_iter(), sink)
    }

    fn send(
        &self,
        paths: impl Iterator<Item = PathBuf>,
        sink: &mut impl FnMut(Event),
    ) -> notify::Result<()> {
        let mut files = BTreeSet::new();
        let (mut index, mut refs) = (false, false);
        for path in paths {
            match self.classify(&path) {
                Kind::File(file) => {
                    files.insert(file);
                }
                Kind::Index => index = true,
                Kind::Refs => refs = true,
                Kind::Skip => {}
            }
        }
        if !files.is_empty() {
            let paths = (files.len() <= MAX_PATHS).then(|| files.into_iter().collect());
            sink(Event::Files { paths });
        }
        if index {
            sink(Event::Index);
        }
        if refs {
            sink(Event::Refs);
        }
        Ok(())
    }

    fn needs_reload(&self, path: &Path) -> bool {
        if !path.starts_with(&self.dir) {
            return false;
        }
        if path.file_name().is_some_and(|name| name == ".gitignore") {
            return true;
        }
        self.config.per_folder
            && if is_folder(path) {
                !self.watched.contains(path) && !self.ignores.is_ignored(path, true)
            } else {
                self.watched.contains(path)
            }
    }

    /// Reads the ignore rules again and, when watching folder by folder, watches the folders that are no longer
    /// ignored or are new, adding the files in them to `found`, and drops the watches of the others.
    fn reload(&mut self, found: &mut BTreeSet<PathBuf>) -> notify::Result<()> {
        let (ignores, folders) = Ignores::load(&self.root, &self.common_dir, &self.dir);
        self.ignores = ignores;
        if !self.config.per_folder {
            return Ok(());
        }
        let folders: BTreeSet<PathBuf> = folders.into_iter().collect();
        for gone in self
            .watched
            .difference(&folders)
            .cloned()
            .collect::<Vec<_>>()
        {
            // The operating system drops the watch of a removed folder by itself, so this may fail.
            let _ = self.watcher.unwatch(&gone);
            self.watched.remove(&gone);
            self.count = self.count.saturating_sub(1);
        }
        for new in folders
            .difference(&self.watched)
            .cloned()
            .collect::<Vec<_>>()
        {
            if !self.watch_folder(&new)? {
                continue;
            }
            // Files may have been written before the watch was added; their changes would be missed.
            if let Ok(entries) = std::fs::read_dir(&new) {
                found.extend(
                    entries
                        .flatten()
                        .filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_dir()))
                        .map(|entry| entry.path()),
                );
            }
            self.watched.insert(new);
        }
        Ok(())
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

fn is_folder(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// `path` with `/` between its parts, as the API names files.
fn slashed(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{expect, expect_none, git, repo, write};
    use crate::{Watch, start_with};

    fn watch(dir: &Path, config: Config) -> (Watch, mpsc::Receiver<Event>) {
        let (sender, events) = mpsc::channel();
        let watch = start_with(Review::new(dir), config, move |event| {
            let _ = sender.send(event);
        });
        // Watches are added on the watching thread; give it time before changing anything.
        std::thread::sleep(std::time::Duration::from_millis(300));
        (watch, events)
    }

    fn files(paths: &[&str]) -> Event {
        Event::Files {
            paths: Some(paths.iter().map(|path| (*path).to_owned()).collect()),
        }
    }

    fn each_mode(test: impl Fn(Config)) {
        for per_folder in [true, false] {
            test(Config {
                per_folder,
                ..Config::new(false)
            });
        }
    }

    #[test]
    fn reports_edited_files() {
        each_mode(|config| {
            let (_tmp, dir) = repo();
            let (_watch, events) = watch(&dir, config);

            write(&dir, "src/a.txt", "two\n");

            assert_eq!(
                expect(&events, |event| matches!(event, Event::Files { .. })),
                files(&["src/a.txt"])
            );
        });
    }

    #[test]
    fn skips_gitignored_files_and_reads() {
        each_mode(|config| {
            let (_tmp, dir) = repo();
            let (_watch, events) = watch(&dir, config);

            write(&dir, "target/out.txt", "rebuilt\n");
            // What vugent itself reads to answer the browser.
            let review = Review::new(&dir);
            review.history().unwrap();
            review.files(&vugent_git::Rev::WorkTree).unwrap();
            review.snapshot().unwrap();

            expect_none(&events, |_| true);
        });
    }

    #[test]
    fn follows_new_folders() {
        each_mode(|config| {
            let (_tmp, dir) = repo();
            let (_watch, events) = watch(&dir, config);

            write(&dir, "new/deep/b.txt", "b\n");
            expect(&events, |event| matches!(event, Event::Files { .. }));
            write(&dir, "new/deep/b.txt", "b, again\n");

            expect(&events, |event| *event == files(&["new/deep/b.txt"]));
        });
    }

    #[test]
    fn follows_changed_ignore_rules() {
        each_mode(|config| {
            let (_tmp, dir) = repo();
            let (_watch, events) = watch(&dir, config);

            write(&dir, ".gitignore", "");
            // Watching folder by folder, the files in folders that are no longer ignored may have changed unseen,
            // so they are listed too.
            let Event::Files { paths: Some(paths) } =
                expect(&events, |event| matches!(event, Event::Files { .. }))
            else {
                panic!("no paths");
            };
            assert_eq!(paths[0], ".gitignore");
            write(&dir, "target/out.txt", "rebuilt\n");

            expect(&events, |event| *event == files(&["target/out.txt"]));
        });
    }

    #[test]
    fn reports_the_index_and_refs() {
        each_mode(|config| {
            let (_tmp, dir) = repo();
            let (_watch, events) = watch(&dir, config);

            write(&dir, "src/a.txt", "two\n");
            git(&dir, &["add", "src/a.txt"]);
            expect(&events, |event| *event == Event::Index);
            git(&dir, &["commit", "--quiet", "-m", "two"]);
            expect(&events, |event| *event == Event::Refs);
            git(&dir, &["switch", "--quiet", "-c", "feature/x"]);

            expect(&events, |event| *event == Event::Refs);
        });
    }

    #[test]
    fn watches_a_subfolder_and_a_linked_worktree() {
        let (_tmp, dir) = repo();
        git(
            &dir,
            &["worktree", "add", "--quiet", "-b", "other", "linked"],
        );
        let linked = dir.join("linked");
        let (_watch, events) = watch(&linked.join("src"), Config::new(false));

        write(&linked, "src/a.txt", "two\n");
        assert_eq!(
            expect(&events, |event| matches!(event, Event::Files { .. })),
            files(&["a.txt"])
        );
        git(&linked, &["commit", "--quiet", "-am", "two"]);
        expect(&events, |event| *event == Event::Refs);
        // The main worktree's HEAD and index are not this worktree's.
        write(&dir, "src/a.txt", "main\n");
        git(&dir, &["add", "src/a.txt"]);

        expect_none(&events, |event| *event == Event::Index);
    }

    #[test]
    fn many_files_are_not_listed() {
        let (_tmp, dir) = repo();
        let (_watch, events) = watch(&dir, Config::new(false));

        for n in 0..=MAX_PATHS {
            write(&dir, &format!("many/{n}.txt"), "");
        }

        expect(&events, |event| *event == Event::Files { paths: None });
    }

    #[test]
    fn polls_when_watches_run_out() {
        let (_tmp, dir) = repo();
        let config = Config {
            per_folder: true,
            max_watches: 2,
            poll_interval: std::time::Duration::from_millis(100),
            ..Config::new(false)
        };
        let (_watch, events) = watch(&dir, config);

        expect(&events, |event| *event == Event::Polling);
        write(&dir, "src/a.txt", "two\n");

        expect(&events, |event| *event == Event::Files { paths: None });
    }

    #[test]
    fn polls_when_watches_run_out_later() {
        let (_tmp, dir) = repo();
        let config = Config {
            per_folder: true,
            // The reviewed folder, src, the git directory and refs.
            max_watches: 4,
            poll_interval: std::time::Duration::from_millis(100),
            ..Config::new(false)
        };
        let (_watch, events) = watch(&dir, config);
        expect_none(&events, |_| true);

        write(&dir, "new/b.txt", "b\n");

        expect(&events, |event| *event == Event::Polling);
    }

    #[test]
    fn classifies_paths() {
        let (_tmp, dir) = repo();
        let config = Config::new(false);
        let (sender, _receiver) = mpsc::channel();
        let watching = Watching::new(&Review::new(&dir), &config, sender).unwrap();
        let git = dir.join(".git");

        assert_eq!(watching.classify(&git.join("index")), Kind::Index);
        assert_eq!(watching.classify(&git.join("index.lock")), Kind::Skip);
        assert_eq!(watching.classify(&git.join("HEAD")), Kind::Refs);
        assert_eq!(watching.classify(&git.join("packed-refs")), Kind::Refs);
        assert_eq!(watching.classify(&git.join("refs/heads/main")), Kind::Refs);
        assert_eq!(
            watching.classify(&git.join("refs/heads/main.lock")),
            Kind::Skip
        );
        assert_eq!(watching.classify(&git.join("ORIG_HEAD")), Kind::Skip);
        assert_eq!(watching.classify(&git.join("objects/ab/cdef")), Kind::Skip);
        assert_eq!(
            watching.classify(&dir.join("src/a.txt")),
            Kind::File("src/a.txt".to_owned())
        );
        assert_eq!(watching.classify(&dir.join("target/out.txt")), Kind::Skip);
        assert_eq!(watching.classify(&dir), Kind::Skip);
    }
}
