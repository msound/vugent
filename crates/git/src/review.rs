//! Reviews of one folder of a repository, each a (base, target) pair from ADR 0002, ADR 0005 and ADR 0006:
//! uncommitted changes, staged or unstaged changes, a whole branch, or a single commit.

use std::collections::{BTreeMap, HashMap};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diff::{self, FileDiff};
use crate::{Error, run, run_stdin, run_with};

/// How a file changed between the base and the target of a review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
    Unchanged,
}

/// A file in the reviewed folder, changed or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// The path relative to the reviewed folder, with `/` separators.
    pub path: String,
    pub status: Status,
    /// Lines added; 0 for binary files and submodules.
    pub added: u32,
    /// Lines removed; 0 for binary files and submodules.
    pub removed: u32,
    pub binary: bool,
    /// The path before a rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// A submodule, or an untracked nested repository, shown as a single entry.
    pub submodule: bool,
    /// Not tracked by git yet.
    pub untracked: bool,
    /// Lines in the target of the review, counted the way git does: 0 for a deleted file, and 1 for a binary file, a
    /// symbolic link or a submodule.
    pub lines: u32,
}

/// Which review to show, as named in the API's `rev` parameter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Rev {
    /// Every uncommitted change, untracked files included, against `HEAD`.
    #[default]
    WorkTree,
    /// The staged changes: the index against `HEAD`.
    Index,
    /// The unstaged changes, untracked files included: the working tree against the index.
    Unstaged,
    /// Everything since the branch left the main branch: the working tree against the merge-base.
    Branch,
    /// A single commit against its first parent, given by its SHA.
    Commit(String),
}

impl Rev {
    /// Reads `worktree`, `index`, `unstaged`, `branch` or a commit's SHA in lowercase hex. Anything else is refused, so that no
    /// other text ever reaches git as a revision.
    pub fn parse(rev: &str) -> Option<Self> {
        match rev {
            "worktree" => Some(Self::WorkTree),
            "index" => Some(Self::Index),
            "unstaged" => Some(Self::Unstaged),
            "branch" => Some(Self::Branch),
            _ if (4..=64).contains(&rev.len())
                && rev
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
            {
                Some(Self::Commit(rev.to_owned()))
            }
            _ => None,
        }
    }
}

/// What a review compares the base against.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    WorkTree,
    Index,
    Commit(String),
}

/// A review resolved to concrete objects: `base` is a commit or the empty tree, or `None` for the index, which is
/// only compared against the working tree.
#[derive(Debug, Clone)]
struct Comparison {
    base: Option<String>,
    target: Target,
}

impl Comparison {
    /// The git command, with its revisions, that compares `base` against the target. Without a base, the working tree
    /// is compared against the index.
    ///
    /// These are plumbing commands: porcelain `git diff` writes a refreshed index whenever it finds a working tree
    /// file whose stat data changed but whose content did not, even with `GIT_OPTIONAL_LOCKS=0`. Plumbing never
    /// writes, but lists such files as modified, so [`Review::changes`] filters them out.
    fn command<'a>(&'a self, base: Option<&'a str>) -> Vec<&'a str> {
        match (&self.target, base) {
            (Target::WorkTree, Some(base)) => vec!["diff-index", base],
            (Target::WorkTree, None) => vec!["diff-files"],
            (Target::Index, base) => ["diff-index", "--cached"].into_iter().chain(base).collect(),
            (Target::Commit(commit), base) => ["diff-tree", "-r"]
                .into_iter()
                .chain(base)
                .chain([commit.as_str()])
                .collect(),
        }
    }
}

/// A changed file, as listed by the `--raw` output of git's diff commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    /// git's status letter, such as `A`, `D`, `M` or `R`.
    pub status: u8,
    pub path: String,
    /// The path before a rename or copy.
    pub old_path: Option<String>,
}

/// A change listed as modified that may only be new stat data.
struct StatCandidate {
    /// Its position in the list of changes.
    index: usize,
    symlink: bool,
    /// The blob id in the base of the comparison.
    old_id: String,
}

/// One side of a file's diff.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Side {
    /// The blob ID, or for a working tree file, the ID it would have; `None` for a working tree file larger than
    /// [`WHOLE_FILE_LIMIT`], which is not read to find it.
    id: Option<String>,
    /// In bytes.
    size: u64,
}

/// Files up to this size on both sides are diffed whole, so that the browser can show every line (ADR 0010).
const WHOLE_FILE_LIMIT: u64 = 1024 * 1024;

/// Context enough for git to include every line of a file up to [`WHOLE_FILE_LIMIT`], even one that is all line
/// breaks, in a single hunk.
const WHOLE_CONTEXT: &str = "-U1048577";

/// git looks for a NUL byte in this many leading bytes to decide whether a file is binary.
const BINARY_PROBE: usize = 8000;

/// The branches tried, in order, when looking for the main branch.
const MAIN_BRANCHES: [&str; 3] = ["origin/HEAD", "main", "master"];

/// Reviews of one folder of a repository. Every call runs git again, so the result is always current. A review never
/// writes to the index or the working tree.
#[derive(Debug, Clone)]
pub struct Review {
    dir: PathBuf,
    /// The main branch given by the user, instead of looking for one.
    main: Option<String>,
}

impl Review {
    /// Reviews `dir`, which must be inside a git working tree. Only files under `dir` are included.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            main: None,
        }
    }

    /// Compares the branch against `main` instead of the first of `origin/HEAD`, `main` and `master` that exists.
    pub fn with_main(mut self, main: impl Into<String>) -> Self {
        self.main = Some(main.into());
        self
    }

    /// The reviewed folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The main branch that branch review compares against, or `None` when there is none.
    pub fn main_branch(&self) -> Result<Option<String>, Error> {
        let candidates = match &self.main {
            Some(main) => vec![main.as_str()],
            None => MAIN_BRANCHES.to_vec(),
        };
        for candidate in candidates {
            // A leading dash would be read as an option.
            if !candidate.starts_with('-') && self.commit(candidate)?.is_some() {
                return Ok(Some(candidate.to_owned()));
            }
        }
        Ok(None)
    }

    /// Lists every file under the reviewed folder in the target of `rev`, plus the files deleted since its base,
    /// sorted by path. Untracked (but not ignored) files are included when the target is the working tree.
    pub fn files(&self, rev: &Rev) -> Result<Vec<FileEntry>, Error> {
        let comparison = self.compare(rev)?;
        let mut files = BTreeMap::new();

        let listing = match &comparison.target {
            Target::Commit(commit) => run(&self.dir, &["ls-tree", "-r", "-z", commit, "--", "."])?,
            Target::WorkTree | Target::Index => {
                run(&self.dir, &["ls-files", "--stage", "-z", "--", "."])?
            }
        };
        for record in split_z(&listing) {
            // ls-files: <mode> <object> <stage>\t<path>; ls-tree: <mode> <type> <object>\t<path>
            let Some((meta, path)) = record.split_once('\t') else {
                continue;
            };
            let submodule = meta.starts_with("160000 ");
            files.insert(path.to_owned(), unchanged(path, submodule));
        }

        let command = comparison.command(comparison.base.as_deref());
        let counts = self.numstat(&command, true)?;
        for change in self.changes(&command, true)? {
            let status = match change.status {
                b'A' => Status::Added,
                b'D' => Status::Deleted,
                b'R' => Status::Renamed,
                // C (copied) only appears with -C.
                b'C' => Status::Added,
                _ => Status::Modified,
            };
            let path = change.path.as_str();
            if let (Status::Renamed, Some(old)) = (status, &change.old_path) {
                files.remove(old);
            }
            let (added, removed, binary) = counts.get(path).copied().unwrap_or_default();
            let entry = files
                .entry(path.to_owned())
                .or_insert_with(|| unchanged(path, false));
            entry.status = status;
            if !entry.submodule {
                entry.added = added;
                entry.removed = removed;
                entry.binary = binary;
            }
            entry.old_path = change.old_path.filter(|_| status == Status::Renamed);
        }

        if comparison.target == Target::WorkTree {
            for entry in self.untracked()? {
                files.insert(entry.path.clone(), entry);
            }
            self.count_work_tree_lines(&mut files)?;
        } else {
            // Diffing the empty tree against the target lists every file as added, with all of its lines.
            let empty = self.empty_tree()?;
            let lines = self.numstat(&comparison.command(Some(&empty)), false)?;
            for entry in files.values_mut() {
                entry.lines = match entry.status {
                    Status::Deleted => 0,
                    _ if entry.submodule || entry.binary => 1,
                    _ => match lines.get(&entry.path) {
                        Some(&(_, _, true)) => 1,
                        Some(&(added, _, false)) => added,
                        None => 0,
                    },
                };
            }
        }

        Ok(files.into_values().collect())
    }

    /// Returns the diff of one file of the review `rev`, given its path relative to the reviewed folder.
    pub fn diff(&self, rev: &Rev, path: &str) -> Result<FileDiff, Error> {
        let comparison = self.compare(rev)?;
        let entry = self
            .files(rev)?
            .into_iter()
            .find(|entry| entry.path == path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let mut diff = FileDiff::empty(&entry);
        if entry.submodule {
            return Ok(diff);
        }

        let sides = self.sides(&comparison, &entry)?;
        // Only files small enough to be diffed whole are highlighted.
        let id = |side: &Option<Side>| {
            side.as_ref()
                .filter(|side| side.size <= WHOLE_FILE_LIMIT)
                .and_then(|side| side.id.clone())
        };
        diff.old_id = id(&sides[0]);
        diff.new_id = id(&sides[1]);

        if entry.status == Status::Unchanged {
            let content = match &comparison.target {
                Target::WorkTree => read(&self.dir.join(path))?,
                // A path starting with ./ is relative to the reviewed folder, not the repository's root.
                Target::Index => self.blob(&format!(":./{path}"))?,
                Target::Commit(commit) => self.blob(&format!("{commit}:./{path}"))?,
            };
            diff.binary = is_binary(&content);
            if !diff.binary {
                diff.hunks = diff::unchanged(&content);
                diff.whole = true;
            }
            return Ok(diff);
        }

        diff.whole = sides
            .iter()
            .flatten()
            .all(|side| side.size <= WHOLE_FILE_LIMIT);
        let context = if diff.whole { WHOLE_CONTEXT } else { "-U3" };
        let output = if entry.untracked {
            run_with(
                &self.dir,
                &[
                    "diff",
                    "--no-index",
                    context,
                    "--no-color",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--",
                    "/dev/null",
                    path,
                ],
                // With --no-index, 1 means the files differ.
                &[0, 1],
            )?
        } else {
            let mut args = comparison.command(comparison.base.as_deref());
            args.extend([
                "-p",
                context,
                "-M",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--relative",
                "--",
            ]);
            args.extend(entry.old_path.as_deref());
            args.push(path);
            run(&self.dir, &args)?
        };
        let (binary, hunks) = diff::parse(&output);
        diff.binary = binary;
        diff.hunks = hunks;
        Ok(diff)
    }

    /// The old and new sides of a file's diff, each `None` where the file does not exist there. An unchanged file
    /// only has a new side, which its lines are shown from.
    fn sides(
        &self,
        comparison: &Comparison,
        entry: &FileEntry,
    ) -> Result<[Option<Side>; 2], Error> {
        let path = entry.path.as_str();
        let old_path = entry.old_path.as_deref().unwrap_or(path);
        // Objects are named relative to the reviewed folder, as in `blob`.
        let has_old =
            !entry.untracked && !matches!(entry.status, Status::Added | Status::Unchanged);
        let old = has_old.then(|| match &comparison.base {
            Some(base) => format!("{base}:./{old_path}"),
            None => format!(":./{old_path}"),
        });
        let new = match (&comparison.target, entry.status) {
            (_, Status::Deleted) => None,
            (Target::WorkTree, _) => {
                let [old] = self.objects([old])?;
                return Ok([old, self.work_tree_side(path)]);
            }
            (Target::Index, _) => Some(format!(":./{path}")),
            (Target::Commit(commit), _) => Some(format!("{commit}:./{path}")),
        };
        self.objects([old, new])
    }

    /// Looks up blobs by names such as `HEAD:./src/main.rs`, giving `None` for a name that is not a blob.
    fn objects<const N: usize>(
        &self,
        names: [Option<String>; N],
    ) -> Result<[Option<Side>; N], Error> {
        // A name with a line break cannot be given to --batch-check, which reads one name per line.
        let names = names.map(|name| name.filter(|name| !name.contains('\n')));
        let input: String = names
            .iter()
            .flatten()
            .map(|name| format!("{name}\n"))
            .collect();
        if input.is_empty() {
            return Ok([const { None }; N]);
        }
        let output = run_stdin(&self.dir, &["cat-file", "--batch-check"], input.as_bytes())?;
        let output = String::from_utf8_lossy(&output);
        let mut answers = output.lines();
        Ok(names.map(|name| {
            name?;
            // <id> blob <size>, or <name> missing.
            let mut fields = answers.next()?.split(' ');
            let (id, kind, size) = (fields.next()?, fields.next()?, fields.next()?);
            let hex = id.bytes().all(|byte| byte.is_ascii_hexdigit());
            (hex && kind == "blob" && fields.next().is_none()).then_some(())?;
            Some(Side {
                id: Some(id.to_owned()),
                size: size.parse().ok()?,
            })
        }))
    }

    /// The working tree side of the file at `path`, or `None` when it cannot be read.
    fn work_tree_side(&self, path: &str) -> Option<Side> {
        let file = self.dir.join(path);
        let meta = std::fs::symlink_metadata(&file).ok()?;
        let size = if meta.file_type().is_symlink() {
            // git stores a symbolic link as its target.
            std::fs::read_link(&file).ok()?.as_os_str().len() as u64
        } else {
            meta.len()
        };
        let id = if size <= WHOLE_FILE_LIMIT {
            let content = read(&file).ok()?;
            Some(self.hash(&content).ok()?)
        } else {
            None
        };
        Some(Side { id, size })
    }

    /// The ID that `content` would have as a blob, in the repository's hash format, without writing it. Filters such
    /// as line ending conversion are not applied, so that the ID changes whenever the file does.
    fn hash(&self, content: &[u8]) -> Result<String, Error> {
        let output = run_stdin(
            &self.dir,
            &["hash-object", "--no-filters", "--stdin"],
            content,
        )?;
        Ok(String::from_utf8_lossy(&output).trim().to_owned())
    }

    /// The content of the blob `id`, which `diff` gave for the file at `path`. A working tree file has no blob, so
    /// when there is none, the file at `path` is read instead, and given only if it still has that ID; otherwise it
    /// changed in the meantime and is not found.
    pub fn content(&self, id: &str, path: &str) -> Result<Vec<u8>, Error> {
        let not_found = || Error::NotFound(format!("{path} at {id}"));
        let hex = matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !hex {
            return Err(not_found());
        }
        if let [Some(_)] = self.objects([Some(id.to_owned())])? {
            return self.blob(id);
        }
        // Only a path inside the reviewed folder is read.
        let inside = Path::new(path)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
        if path.is_empty() || !inside {
            return Err(not_found());
        }
        let content = read(&self.dir.join(path)).map_err(|_| not_found())?;
        if self.hash(&content)? != id {
            return Err(not_found());
        }
        Ok(content)
    }

    /// Resolves `rev` to the objects it compares.
    fn compare(&self, rev: &Rev) -> Result<Comparison, Error> {
        let head = || -> Result<Option<String>, Error> {
            match self.commit("HEAD")? {
                Some(head) => Ok(Some(head)),
                None => self.empty_tree().map(Some),
            }
        };
        Ok(match rev {
            Rev::WorkTree => Comparison {
                base: head()?,
                target: Target::WorkTree,
            },
            Rev::Index => Comparison {
                base: head()?,
                target: Target::Index,
            },
            Rev::Unstaged => Comparison {
                base: None,
                target: Target::WorkTree,
            },
            Rev::Branch => Comparison {
                base: Some(
                    self.branch_base()?
                        .ok_or_else(|| Error::NotFound("the branch".to_owned()))?,
                ),
                target: Target::WorkTree,
            },
            Rev::Commit(sha) => {
                let commit = self
                    .commit(sha)?
                    .ok_or_else(|| Error::NotFound(format!("commit {sha}")))?;
                let base = match self.commit(&format!("{commit}^1"))? {
                    Some(parent) => parent,
                    // The repository's first commit.
                    None => self.empty_tree()?,
                };
                Comparison {
                    base: Some(base),
                    target: Target::Commit(commit),
                }
            }
        })
    }

    /// The full SHA of the commit that `rev` names, or `None` when there is no such commit.
    pub(crate) fn commit(&self, rev: &str) -> Result<Option<String>, Error> {
        // --verify --quiet exits with 1 and no message when the revision does not exist, and 128 outside a
        // repository.
        let out = run_with(
            &self.dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{rev}^{{commit}}"),
            ],
            &[0, 1],
        )?;
        let sha = String::from_utf8_lossy(&out).trim().to_owned();
        Ok(Some(sha).filter(|sha| !sha.is_empty()))
    }

    /// Where the current branch left the main branch, or `None` when there is no main branch, no common history, or
    /// `HEAD` is on the main branch itself.
    pub(crate) fn branch_base(&self) -> Result<Option<String>, Error> {
        let (Some(head), Some(main)) = (self.commit("HEAD")?, self.main_branch()?) else {
            return Ok(None);
        };
        // merge-base exits with 1 when the two have no common history.
        let out = run_with(&self.dir, &["merge-base", &head, &main], &[0, 1])?;
        let base = String::from_utf8_lossy(&out).trim().to_owned();
        Ok(Some(base).filter(|base| !base.is_empty() && *base != head))
    }

    /// The empty tree, the base when there is no commit to compare against.
    pub(crate) fn empty_tree(&self) -> Result<String, Error> {
        // Hashing without -w writes nothing. Its stdin is empty, which is the empty tree, in SHA-1 and SHA-256 repos.
        let empty = run(&self.dir, &["hash-object", "-t", "tree", "--stdin"])?;
        Ok(String::from_utf8_lossy(&empty).trim().to_owned())
    }

    /// Untracked files that are not ignored, each shown as added.
    fn untracked(&self) -> Result<Vec<FileEntry>, Error> {
        let others = run(
            &self.dir,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                ".",
            ],
        )?;
        split_z(&others)
            .into_iter()
            .map(|path| {
                Ok(match path.strip_suffix('/') {
                    // git lists an untracked nested repository as a folder and does not look inside it.
                    Some(repo) => FileEntry {
                        status: Status::Added,
                        untracked: true,
                        lines: 1,
                        ..unchanged(repo, true)
                    },
                    None => {
                        let (lines, binary) = count_lines(&self.dir.join(path))?;
                        FileEntry {
                            status: Status::Added,
                            added: lines,
                            binary,
                            untracked: true,
                            lines: if binary { 1 } else { lines },
                            ..unchanged(path, false)
                        }
                    }
                })
            })
            .collect()
    }

    /// Counts the lines of every tracked file in the working tree. Untracked files are counted when they are listed.
    fn count_work_tree_lines(&self, files: &mut BTreeMap<String, FileEntry>) -> Result<(), Error> {
        for entry in files.values_mut().filter(|entry| !entry.untracked) {
            entry.lines = match entry.status {
                Status::Deleted => 0,
                _ if entry.submodule || entry.binary => 1,
                _ => match count_lines(&self.dir.join(&entry.path)) {
                    Ok((_, true)) => 1,
                    Ok((lines, false)) => lines,
                    // Added to the index and then deleted: git shows no change against HEAD. Against the index it is
                    // deleted, and handled above.
                    Err(Error::Read { source, .. }) if source.kind() == ErrorKind::NotFound => 0,
                    Err(err) => return Err(err),
                },
            };
        }
        Ok(())
    }

    /// The content of a blob, such as `:./src/lib.rs` in the index or `<commit>:./src/lib.rs`.
    fn blob(&self, object: &str) -> Result<Vec<u8>, Error> {
        run(&self.dir, &["cat-file", "blob", object])
    }

    /// The changed tracked files that the diff `command` lists, leaving out working tree files whose stat data
    /// changed but whose content and mode did not.
    pub(crate) fn changes(&self, command: &[&str], renames: bool) -> Result<Vec<Change>, Error> {
        let mut args = command.to_vec();
        args.extend([
            if renames { "-M" } else { "--no-renames" },
            "--raw",
            "-z",
            "--relative",
            "--",
            ".",
        ]);
        let out = run(&self.dir, &args)?;
        let mut changes = Vec::new();
        let mut candidates = Vec::new();
        let mut records = split_z(&out).into_iter();
        while let Some(meta) = records.next() {
            // :<old mode> <new mode> <old id> <new id> <status>, then the path, or the old and new paths of a rename
            // or copy.
            let fields: Vec<_> = meta.trim_start_matches(':').split(' ').collect();
            let [old_mode, new_mode, old_id, new_id, status] = fields[..] else {
                break;
            };
            let status = status.as_bytes().first().copied().unwrap_or(b'M');
            let old_path = if matches!(status, b'R' | b'C') {
                records.next()
            } else {
                None
            };
            let Some(path) = records.next() else { break };
            // The id of a working tree file is unknown (all zeros) unless the index's stat data says it is clean.
            // Submodules are left alone: theirs is unknown when they have changes of their own.
            if status == b'M'
                && old_mode == new_mode
                && new_mode != "160000"
                && new_id.bytes().all(|byte| byte == b'0')
            {
                candidates.push(StatCandidate {
                    index: changes.len(),
                    symlink: new_mode == "120000",
                    old_id: old_id.to_owned(),
                });
            }
            changes.push(Change {
                status,
                path: path.to_owned(),
                old_path: old_path.map(str::to_owned),
            });
        }

        let mut same = vec![false; changes.len()];
        // hash-object reads its paths one per line, and follows symbolic links.
        let (links, files): (Vec<_>, Vec<_>) = candidates
            .iter()
            .filter(|candidate| !changes[candidate.index].path.contains('\n'))
            .partition(|candidate| candidate.symlink);
        if !files.is_empty() {
            let paths: String = files
                .iter()
                .map(|candidate| format!("{}\n", changes[candidate.index].path))
                .collect();
            // Without -w nothing is written. Hashing a path applies its clean filters, as adding it would.
            let ids = run_stdin(
                &self.dir,
                &["hash-object", "--stdin-paths"],
                paths.as_bytes(),
            )?;
            for (candidate, id) in files.iter().zip(String::from_utf8_lossy(&ids).lines()) {
                same[candidate.index] = id == candidate.old_id;
            }
        }
        for candidate in links {
            // read() gives a symbolic link's target, which is what git stores.
            if let Ok(target) = read(&self.dir.join(&changes[candidate.index].path)) {
                same[candidate.index] = target == self.blob(&candidate.old_id)?;
            }
        }
        let mut same = same.into_iter();
        changes.retain(|_| !same.next().unwrap_or(false));
        Ok(changes)
    }

    /// Line counts of changed tracked files that the diff `command` lists, keyed by their path:
    /// (added, removed, binary).
    fn numstat(
        &self,
        command: &[&str],
        renames: bool,
    ) -> Result<HashMap<String, (u32, u32, bool)>, Error> {
        let mut args = command.to_vec();
        args.extend([
            if renames { "-M" } else { "--no-renames" },
            "--numstat",
            "-z",
            "--relative",
            "--",
            ".",
        ]);
        let out = run(&self.dir, &args)?;
        let mut counts = HashMap::new();
        let mut records = split_z(&out).into_iter();
        while let Some(record) = records.next() {
            // <added>\t<removed>\t<path>, or <added>\t<removed>\t followed by the old and new paths for a rename.
            let mut fields = record.splitn(3, '\t');
            let (Some(added), Some(removed), Some(path)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let path = if path.is_empty() {
                records.next();
                match records.next() {
                    Some(new) => new,
                    None => break,
                }
            } else {
                path
            };
            let binary = added == "-" && removed == "-";
            counts.insert(
                path.to_owned(),
                (
                    added.parse().unwrap_or(0),
                    removed.parse().unwrap_or(0),
                    binary,
                ),
            );
        }
        Ok(counts)
    }
}

fn unchanged(path: &str, submodule: bool) -> FileEntry {
    FileEntry {
        path: path.to_owned(),
        status: Status::Unchanged,
        added: 0,
        removed: 0,
        binary: false,
        old_path: None,
        submodule,
        untracked: false,
        lines: 0,
    }
}

/// Splits NUL-terminated output, as printed by git's `-z` options, into its records.
pub(crate) fn split_z(out: &[u8]) -> Vec<&str> {
    out.split(|&byte| byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| std::str::from_utf8(record).unwrap_or_default())
        .filter(|record| !record.is_empty())
        .collect()
}

pub(crate) fn is_binary(content: &[u8]) -> bool {
    content[..content.len().min(BINARY_PROBE)].contains(&0)
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    // A symbolic link is stored by git as its target, so show the target rather than following it.
    if let Ok(target) = std::fs::read_link(path) {
        return Ok(target.to_string_lossy().into_owned().into_bytes());
    }
    std::fs::read(path).map_err(|source| Error::Read {
        path: path.display().to_string(),
        source,
    })
}

/// Counts the lines of a working tree file the way `git diff` would, and tells whether it is binary.
fn count_lines(path: &Path) -> Result<(u32, bool), Error> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Ok((1, false));
    }
    let error = |source| Error::Read {
        path: path.display().to_string(),
        source,
    };
    let mut file = std::fs::File::open(path).map_err(error)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content).map_err(error)?;
    if is_binary(&content) {
        return Ok((0, true));
    }
    let newlines = content.iter().filter(|&&byte| byte == b'\n').count();
    let unterminated = usize::from(content.last().is_some_and(|&byte| byte != b'\n'));
    Ok((
        u32::try_from(newlines + unterminated).unwrap_or(u32::MAX),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{git, make_stale, write};
    use crate::{FileDiff, LineKind};

    /// A repository with one commit: `a.txt`, `src/lib.rs` and `src/old.rs`.
    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        git(dir, &["init", "--quiet"]);
        write(dir, "a.txt", b"one\ntwo\nthree\n");
        write(dir, "src/lib.rs", b"fn main() {}\n");
        write(
            dir,
            "src/old.rs",
            b"// a file long enough\n// to be found\n// as renamed\n",
        );
        git(dir, &["add", "."]);
        git(dir, &["commit", "--quiet", "-m", "init"]);
        tmp
    }

    fn find<'a>(files: &'a [FileEntry], path: &str) -> &'a FileEntry {
        files
            .iter()
            .find(|entry| entry.path == path)
            .unwrap_or_else(|| panic!("{path} missing from {files:#?}"))
    }

    fn kinds(diff: &FileDiff) -> Vec<(LineKind, &str)> {
        diff.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .map(|line| (line.kind, line.text.as_str()))
            .collect()
    }

    #[test]
    fn a_clean_repo_lists_every_file_as_unchanged() {
        let tmp = repo();

        let files = Review::new(tmp.path()).files(&Rev::WorkTree).unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "src/lib.rs", "src/old.rs"]);
        assert!(files.iter().all(|entry| entry.status == Status::Unchanged));
    }

    #[test]
    fn lists_every_kind_of_change() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\n2\nthree\nfour\n");
        write(dir, "new.txt", b"x\ny");
        write(dir, "staged.txt", b"s\n");
        git(dir, &["add", "staged.txt"]);
        git(dir, &["mv", "src/old.rs", "src/new.rs"]);
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();
        write(dir, "image.bin", b"\0\x01\x02");
        write(dir, "ignored.log", b"noise\n");
        write(dir, ".gitignore", b"*.log\n");

        let files = Review::new(dir).files(&Rev::WorkTree).unwrap();

        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.untracked),
            (Status::Modified, 2, 1, false)
        );
        let new = find(&files, "new.txt");
        assert_eq!(
            (new.status, new.added, new.untracked),
            (Status::Added, 2, true)
        );
        assert_eq!(find(&files, "staged.txt").status, Status::Added);
        let renamed = find(&files, "src/new.rs");
        assert_eq!(renamed.status, Status::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("src/old.rs"));
        assert!(!files.iter().any(|entry| entry.path == "src/old.rs"));
        assert_eq!(find(&files, "src/lib.rs").status, Status::Deleted);
        let image = find(&files, "image.bin");
        assert!(image.binary);
        assert_eq!(image.added, 0);
        assert!(!files.iter().any(|entry| entry.path == "ignored.log"));
    }

    #[test]
    fn counts_the_lines_of_every_file_in_the_working_tree() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\ntwo");
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();
        write(dir, "new.txt", b"x\ny\nz\n");
        write(dir, "image.bin", b"\0\x01\x02");
        write(dir, "gone.txt", b"g\n");
        git(dir, &["add", "gone.txt"]);
        std::fs::remove_file(dir.join("gone.txt")).unwrap();

        let files = Review::new(dir).files(&Rev::WorkTree).unwrap();

        let lines = |path| find(&files, path).lines;
        assert_eq!(lines("a.txt"), 2);
        assert_eq!(lines("src/lib.rs"), 0);
        assert_eq!(lines("src/old.rs"), 3);
        assert_eq!(lines("new.txt"), 3);
        assert_eq!(lines("image.bin"), 1);
        assert_eq!(lines("gone.txt"), 0);
    }

    #[test]
    fn binary_tracked_files_have_no_line_counts() {
        let tmp = repo();
        write(tmp.path(), "blob.bin", b"\0a");
        git(tmp.path(), &["add", "blob.bin"]);
        git(tmp.path(), &["commit", "--quiet", "-m", "blob"]);
        write(tmp.path(), "blob.bin", b"\0b");

        let files = Review::new(tmp.path()).files(&Rev::WorkTree).unwrap();

        let blob = find(&files, "blob.bin");
        assert_eq!(
            (blob.status, blob.binary, blob.added, blob.removed),
            (Status::Modified, true, 0, 0)
        );
    }

    #[test]
    fn is_limited_to_the_reviewed_folder() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"changed\n");
        write(tmp.path(), "src/extra.rs", b"\n");

        let files = Review::new(tmp.path().join("src"))
            .files(&Rev::WorkTree)
            .unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["extra.rs", "lib.rs", "old.rs"]);
    }

    #[test]
    fn a_repo_without_commits_shows_everything_as_added() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "staged.txt", b"s\n");
        git(tmp.path(), &["add", "staged.txt"]);
        write(tmp.path(), "loose.txt", b"l\n");

        let review = Review::new(tmp.path());
        let files = review.files(&Rev::WorkTree).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|entry| entry.status == Status::Added));
        let diff = review.diff(&Rev::WorkTree, "staged.txt").unwrap();
        assert_eq!(kinds(&diff), [(LineKind::Added, "s")]);
    }

    #[test]
    fn works_on_a_detached_head() {
        let tmp = repo();
        git(tmp.path(), &["checkout", "--quiet", "--detach"]);
        write(tmp.path(), "a.txt", b"one\n");

        let files = Review::new(tmp.path()).files(&Rev::WorkTree).unwrap();

        assert_eq!(find(&files, "a.txt").status, Status::Modified);
    }

    #[test]
    fn submodules_are_single_entries() {
        let tmp = repo();
        let dir = tmp.path();
        // Adding a checked-out repository records it as a submodule (a gitlink), as `git submodule add` does.
        let lib = dir.join("vendor/lib");
        std::fs::create_dir_all(&lib).unwrap();
        git(&lib, &["init", "--quiet"]);
        write(&lib, "code.rs", b"\n");
        git(&lib, &["add", "."]);
        git(&lib, &["commit", "--quiet", "-m", "lib"]);
        git(dir, &["add", "vendor/lib"]);
        let nested = dir.join("nested");
        std::fs::create_dir(&nested).unwrap();
        git(&nested, &["init", "--quiet"]);
        write(&nested, "inside.txt", b"hidden\n");

        let review = Review::new(dir);
        let files = review.files(&Rev::WorkTree).unwrap();

        let lib = find(&files, "vendor/lib");
        assert!(lib.submodule && !lib.binary);
        assert_eq!(lib.status, Status::Added);
        let nested = find(&files, "nested");
        assert!(nested.submodule && nested.untracked);
        assert_eq!((lib.lines, nested.lines), (1, 1));
        assert!(!files.iter().any(|entry| entry.path.starts_with("nested/")));
        let diff = review.diff(&Rev::WorkTree, "vendor/lib").unwrap();
        assert!(diff.submodule && diff.hunks.is_empty());
    }

    #[test]
    fn diffs_a_modified_file() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"one\n2\nthree\n");

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "a.txt")
            .unwrap();

        assert_eq!(diff.status, Status::Modified);
        assert!(!diff.binary);
        assert_eq!(
            kinds(&diff),
            [
                (LineKind::Context, "one"),
                (LineKind::Removed, "two"),
                (LineKind::Added, "2"),
                (LineKind::Context, "three"),
            ]
        );
        let lines = &diff.hunks[0].lines;
        assert_eq!((lines[1].old_line, lines[1].new_line), (Some(2), None));
        assert_eq!((lines[2].old_line, lines[2].new_line), (None, Some(2)));
        assert_eq!((lines[3].old_line, lines[3].new_line), (Some(3), Some(3)));
    }

    #[test]
    fn diffs_untracked_and_deleted_files_in_full() {
        let tmp = repo();
        write(tmp.path(), "src/new.txt", b"a\nb\n");
        std::fs::remove_file(tmp.path().join("a.txt")).unwrap();
        let review = Review::new(tmp.path());

        let added = review.diff(&Rev::WorkTree, "src/new.txt").unwrap();
        let deleted = review.diff(&Rev::WorkTree, "a.txt").unwrap();

        assert_eq!(
            kinds(&added),
            [(LineKind::Added, "a"), (LineKind::Added, "b")]
        );
        assert_eq!(
            kinds(&deleted),
            [
                (LineKind::Removed, "one"),
                (LineKind::Removed, "two"),
                (LineKind::Removed, "three"),
            ]
        );
    }

    #[test]
    fn diffs_a_renamed_file_against_its_old_path() {
        let tmp = repo();
        git(tmp.path(), &["mv", "src/old.rs", "src/new.rs"]);
        write(
            tmp.path(),
            "src/new.rs",
            b"// a file long enough\n// to be found\n// as renamed!\n",
        );

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "src/new.rs")
            .unwrap();

        assert_eq!(diff.status, Status::Renamed);
        assert_eq!(diff.old_path.as_deref(), Some("src/old.rs"));
        assert_eq!(
            kinds(&diff),
            [
                (LineKind::Context, "// a file long enough"),
                (LineKind::Context, "// to be found"),
                (LineKind::Removed, "// as renamed"),
                (LineKind::Added, "// as renamed!"),
            ]
        );
    }

    /// A file of `count` numbered lines, with line `changed` replaced when given.
    fn numbered(count: usize, changed: Option<usize>) -> Vec<u8> {
        (1..=count)
            .map(|line| match changed {
                Some(changed) if changed == line => "changed\n".to_owned(),
                _ => format!("{line}\n"),
            })
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn diffs_every_line_of_a_file() {
        let tmp = repo();
        write(tmp.path(), "long.txt", &numbered(100, None));
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "--quiet", "-m", "long"]);
        write(tmp.path(), "long.txt", &numbered(100, Some(50)));
        let review = Review::new(tmp.path());

        for rev in [Rev::WorkTree, Rev::Unstaged] {
            let diff = review.diff(&rev, "long.txt").unwrap();

            assert!(diff.whole);
            assert_eq!(diff.hunks.len(), 1);
            let lines = &diff.hunks[0].lines;
            assert_eq!(lines.len(), 101);
            assert_eq!((lines[0].old_line, lines[0].new_line), (Some(1), Some(1)));
            assert_eq!(lines[49].kind, LineKind::Removed);
            assert_eq!(lines[50].kind, LineKind::Added);
            assert_eq!(lines[100].new_line, Some(100));
        }

        git(tmp.path(), &["commit", "--quiet", "-am", "change"]);
        let commit = Rev::Commit(head(tmp.path()));
        let diff = review.diff(&commit, "long.txt").unwrap();
        assert!(diff.whole);
        assert_eq!(diff.hunks[0].lines.len(), 101);
    }

    #[test]
    fn diffs_every_line_of_untracked_files() {
        let tmp = repo();
        write(tmp.path(), "new.txt", &numbered(20, None));

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "new.txt")
            .unwrap();

        assert!(diff.whole);
        assert_eq!(diff.hunks[0].lines.len(), 20);
    }

    #[test]
    fn large_files_keep_a_few_lines_of_context() {
        let tmp = repo();
        // Larger than a mebibyte only after the change.
        let count = 200_000;
        write(tmp.path(), "large.txt", &numbered(count, None));
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "--quiet", "-m", "large"]);
        let mut content = numbered(count, Some(1000));
        content.extend(std::iter::repeat_n(b'x', 1024 * 1024));
        content.push(b'\n');
        write(tmp.path(), "large.txt", &content);

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "large.txt")
            .unwrap();

        assert!(!diff.whole);
        assert_eq!(diff.hunks.len(), 2);
        assert_eq!(diff.hunks[0].lines.len(), 8);
    }

    #[test]
    fn gives_the_blob_id_of_each_side() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\n2\nthree\n");
        let review = Review::new(dir);
        let old = git(dir, &["rev-parse", "HEAD:a.txt"]);
        let new = git(dir, &["hash-object", "a.txt"]);

        let diff = review.diff(&Rev::WorkTree, "a.txt").unwrap();
        assert_eq!(diff.old_id.as_deref(), Some(old.trim()));
        assert_eq!(diff.new_id.as_deref(), Some(new.trim()));

        git(dir, &["add", "a.txt"]);
        let staged = review.diff(&Rev::Index, "a.txt").unwrap();
        assert_eq!(staged.new_id.as_deref(), Some(new.trim()));

        // An unchanged file only has a new side, and a deleted one only an old side.
        let unchanged = review.diff(&Rev::WorkTree, "src/lib.rs").unwrap();
        assert_eq!(unchanged.old_id, None);
        assert!(unchanged.new_id.is_some());
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();
        let deleted = review.diff(&Rev::WorkTree, "src/lib.rs").unwrap();
        assert!(deleted.old_id.is_some());
        assert_eq!(deleted.new_id, None);
    }

    #[test]
    fn reads_blobs_and_working_tree_files_by_id() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\n2\nthree\n");
        let review = Review::new(dir);
        let diff = review.diff(&Rev::WorkTree, "a.txt").unwrap();
        let (old, new) = (diff.old_id.unwrap(), diff.new_id.unwrap());

        assert_eq!(review.content(&old, "a.txt").unwrap(), b"one\ntwo\nthree\n");
        assert_eq!(review.content(&new, "a.txt").unwrap(), b"one\n2\nthree\n");

        // The file changed since its diff was asked for.
        write(dir, "a.txt", b"other\n");
        assert!(matches!(
            review.content(&new, "a.txt"),
            Err(Error::NotFound(_))
        ));
        // Only blobs, by full ID, and only files inside the reviewed folder.
        let commit = head(dir);
        for (id, path) in [
            (commit.as_str(), "a.txt"),
            (&old[..7], "a.txt"),
            (new.as_str(), "../a.txt"),
            (new.as_str(), "/etc/passwd"),
            (new.as_str(), ""),
        ] {
            assert!(
                matches!(review.content(id, path), Err(Error::NotFound(_))),
                "{id} {path}"
            );
        }
    }

    #[test]
    fn large_files_have_no_blob_ids() {
        let tmp = repo();
        write(tmp.path(), "large.txt", &vec![b'x'; 1024 * 1024 + 1]);

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "large.txt")
            .unwrap();

        assert_eq!((diff.old_id, diff.new_id), (None, None));
    }

    #[test]
    fn binary_files_have_no_lines() {
        let tmp = repo();
        write(tmp.path(), "image.bin", b"\0\x01");

        let diff = Review::new(tmp.path())
            .diff(&Rev::WorkTree, "image.bin")
            .unwrap();

        assert!(diff.binary);
        assert!(diff.hunks.is_empty());
    }

    #[test]
    fn unchanged_files_are_shown_as_context() {
        let tmp = repo();

        let diff = Review::new(tmp.path().join("src"))
            .diff(&Rev::WorkTree, "lib.rs")
            .unwrap();

        assert_eq!(kinds(&diff), [(LineKind::Context, "fn main() {}")]);
    }

    #[test]
    fn paths_outside_the_review_are_not_found() {
        let tmp = repo();

        let err = Review::new(tmp.path().join("src"))
            .diff(&Rev::WorkTree, "../a.txt")
            .unwrap_err();

        assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    }

    #[test]
    fn never_writes_to_the_index() {
        let tmp = repo();
        write(tmp.path(), "a.txt", b"changed\n");
        write(tmp.path(), "new.txt", b"new\n");
        make_stale(tmp.path(), "src/lib.rs");
        let index = tmp.path().join(".git/index");
        let before = std::fs::read(&index).unwrap();

        let review = Review::new(tmp.path());
        review.history().unwrap();
        for rev in [Rev::WorkTree, Rev::Unstaged, Rev::Index] {
            review.files(&rev).unwrap();
            review.diff(&rev, "src/lib.rs").unwrap();
        }
        review.diff(&Rev::WorkTree, "a.txt").unwrap();
        review.diff(&Rev::WorkTree, "new.txt").unwrap();

        assert_eq!(std::fs::read(&index).unwrap(), before);
    }

    #[test]
    fn files_whose_stat_data_alone_changed_are_unchanged() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "image.bin", b"\0\x01\x02");
        git(dir, &["add", "image.bin"]);
        git(dir, &["commit", "--quiet", "-m", "image"]);
        make_stale(dir, "src/lib.rs");
        make_stale(dir, "image.bin");
        write(dir, "a.txt", b"changed\n");

        for rev in [Rev::WorkTree, Rev::Unstaged] {
            let files = Review::new(dir).files(&rev).unwrap();

            assert_eq!(
                find(&files, "src/lib.rs").status,
                Status::Unchanged,
                "{rev:?}"
            );
            assert_eq!(
                find(&files, "image.bin").status,
                Status::Unchanged,
                "{rev:?}"
            );
            assert_eq!(find(&files, "a.txt").status, Status::Modified, "{rev:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_made_again_is_unchanged() {
        let tmp = repo();
        let dir = tmp.path();
        std::os::unix::fs::symlink("a.txt", dir.join("link")).unwrap();
        git(dir, &["add", "link"]);
        git(dir, &["commit", "--quiet", "-m", "link"]);
        make_stale(dir, "a.txt");
        // Making the link again gives it new stat data; set_modified would follow it instead.
        std::fs::remove_file(dir.join("link")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::os::unix::fs::symlink("a.txt", dir.join("link")).unwrap();

        let files = Review::new(dir).files(&Rev::WorkTree).unwrap();

        assert_eq!(find(&files, "link").status, Status::Unchanged);
    }

    #[cfg(unix)]
    #[test]
    fn a_mode_change_alone_is_modified() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = repo();
        let dir = tmp.path();
        let lib = dir.join("src/lib.rs");
        std::fs::set_permissions(&lib, std::fs::Permissions::from_mode(0o755)).unwrap();

        let files = Review::new(dir).files(&Rev::WorkTree).unwrap();

        let entry = find(&files, "src/lib.rs");
        assert_eq!(
            (entry.status, entry.added, entry.removed),
            (Status::Modified, 0, 0)
        );
    }

    fn head(dir: &Path) -> String {
        git(dir, &["rev-parse", "HEAD"]).trim().to_owned()
    }

    #[test]
    fn parses_revs() {
        assert_eq!(Rev::parse("worktree"), Some(Rev::WorkTree));
        assert_eq!(Rev::parse("index"), Some(Rev::Index));
        assert_eq!(Rev::parse("unstaged"), Some(Rev::Unstaged));
        assert_eq!(Rev::parse("branch"), Some(Rev::Branch));
        assert_eq!(
            Rev::parse("0123abcd"),
            Some(Rev::Commit("0123abcd".to_owned()))
        );
        for bad in [
            "",
            "HEAD",
            "abc",
            "--all",
            "ABCDEF12",
            "abc1234^",
            &"a".repeat(65),
        ] {
            assert_eq!(Rev::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_index_shows_only_staged_changes() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\n");
        git(dir, &["add", "a.txt"]);
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\nfive\n");
        write(dir, "src/lib.rs", b"fn main() { }\n");
        write(dir, "new.txt", b"n\n");

        let review = Review::new(dir);
        let files = review.files(&Rev::Index).unwrap();

        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.lines),
            (Status::Modified, 1, 0, 4)
        );
        assert_eq!(find(&files, "src/lib.rs").status, Status::Unchanged);
        assert!(!files.iter().any(|entry| entry.path == "new.txt"));
        let diff = review.diff(&Rev::Index, "a.txt").unwrap();
        assert_eq!(kinds(&diff).last(), Some(&(LineKind::Added, "four")));
        let lib = review.diff(&Rev::Index, "src/lib.rs").unwrap();
        assert_eq!(kinds(&lib), [(LineKind::Context, "fn main() {}")]);
    }

    #[test]
    fn unstaged_shows_only_changes_since_the_index() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\n");
        git(dir, &["add", "a.txt"]);
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\nfive\n");
        write(dir, "staged.txt", b"s\n");
        git(dir, &["add", "staged.txt"]);
        write(dir, "new.txt", b"n\n");
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();

        let review = Review::new(dir);
        let files = review.files(&Rev::Unstaged).unwrap();

        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.lines),
            (Status::Modified, 1, 0, 5)
        );
        let staged = find(&files, "staged.txt");
        assert_eq!((staged.status, staged.lines), (Status::Unchanged, 1));
        let new = find(&files, "new.txt");
        assert_eq!((new.status, new.untracked), (Status::Added, true));
        assert_eq!(find(&files, "src/lib.rs").status, Status::Deleted);
        let diff = review.diff(&Rev::Unstaged, "a.txt").unwrap();
        assert_eq!(
            kinds(&diff),
            [
                (LineKind::Context, "one"),
                (LineKind::Context, "two"),
                (LineKind::Context, "three"),
                (LineKind::Context, "four"),
                (LineKind::Added, "five"),
            ]
        );
        let unchanged = review.diff(&Rev::Unstaged, "staged.txt").unwrap();
        assert_eq!(kinds(&unchanged), [(LineKind::Context, "s")]);
    }

    #[test]
    fn unstaged_works_in_a_repo_without_commits() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        write(tmp.path(), "staged.txt", b"s\n");
        git(tmp.path(), &["add", "staged.txt"]);
        write(tmp.path(), "loose.txt", b"l\n");

        let files = Review::new(tmp.path()).files(&Rev::Unstaged).unwrap();

        assert_eq!(find(&files, "staged.txt").status, Status::Unchanged);
        assert_eq!(find(&files, "loose.txt").status, Status::Added);
    }

    #[test]
    fn a_commit_shows_the_tree_as_it_was_and_only_its_changes() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "a.txt", b"one\n2\nthree\n");
        std::fs::remove_file(dir.join("src/lib.rs")).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "--quiet", "-m", "change"]);
        let change = head(dir);
        write(dir, "later.txt", b"later\n");
        write(dir, "src/old.rs", b"rewritten\n");
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "--quiet", "-m", "later"]);
        write(dir, "a.txt", b"uncommitted\n");

        let review = Review::new(dir);
        let rev = Rev::Commit(change);
        let files = review.files(&rev).unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "src/lib.rs", "src/old.rs"]);
        let a = find(&files, "a.txt");
        assert_eq!(
            (a.status, a.added, a.removed, a.lines),
            (Status::Modified, 1, 1, 3)
        );
        let lib = find(&files, "src/lib.rs");
        assert_eq!((lib.status, lib.lines), (Status::Deleted, 0));
        let old = find(&files, "src/old.rs");
        assert_eq!((old.status, old.lines), (Status::Unchanged, 3));
        let diff = review.diff(&rev, "src/old.rs").unwrap();
        assert_eq!(
            kinds(&diff)[0],
            (LineKind::Context, "// a file long enough")
        );
        let diff = review.diff(&rev, "a.txt").unwrap();
        assert!(kinds(&diff).contains(&(LineKind::Added, "2")));
    }

    #[test]
    fn the_first_commit_is_all_added() {
        let tmp = repo();
        let first = head(tmp.path());

        let files = Review::new(tmp.path()).files(&Rev::Commit(first)).unwrap();

        assert!(files.iter().all(|entry| entry.status == Status::Added));
        assert_eq!(find(&files, "a.txt").added, 3);
    }

    #[test]
    fn a_commit_works_in_a_subfolder() {
        let tmp = repo();
        let first = head(tmp.path());

        let review = Review::new(tmp.path().join("src"));
        let files = review.files(&Rev::Commit(first.clone())).unwrap();

        let paths: Vec<_> = files.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["lib.rs", "old.rs"]);
        assert_eq!(find(&files, "lib.rs").lines, 1);
        let diff = review.diff(&Rev::Commit(first), "lib.rs").unwrap();
        assert_eq!(kinds(&diff), [(LineKind::Added, "fn main() {}")]);
    }

    #[test]
    fn a_merge_commit_is_compared_against_its_first_parent() {
        let tmp = repo();
        let dir = tmp.path();
        git(dir, &["checkout", "--quiet", "-b", "side"]);
        write(dir, "side.txt", b"s\n");
        git(dir, &["add", "side.txt"]);
        git(dir, &["commit", "--quiet", "-m", "side"]);
        git(dir, &["checkout", "--quiet", "main"]);
        write(dir, "main.txt", b"m\n");
        git(dir, &["add", "main.txt"]);
        git(dir, &["commit", "--quiet", "-m", "main"]);
        git(dir, &["merge", "--quiet", "--no-ff", "-m", "merge", "side"]);

        let files = Review::new(dir).files(&Rev::Commit(head(dir))).unwrap();

        assert_eq!(find(&files, "side.txt").status, Status::Added);
        assert_eq!(find(&files, "main.txt").status, Status::Unchanged);
    }

    #[test]
    fn an_unknown_commit_is_not_found() {
        let tmp = repo();

        let err = Review::new(tmp.path())
            .files(&Rev::Commit("0".repeat(40)))
            .unwrap_err();

        assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    }

    #[test]
    fn the_branch_spans_its_commits_and_uncommitted_work() {
        let tmp = repo();
        let dir = tmp.path();
        git(dir, &["checkout", "--quiet", "-b", "feature"]);
        write(dir, "a.txt", b"one\ntwo\nthree\nfour\n");
        git(dir, &["commit", "--quiet", "-am", "four"]);
        write(dir, "new.txt", b"n\n");

        let review = Review::new(dir);
        let files = review.files(&Rev::Branch).unwrap();

        assert_eq!(find(&files, "a.txt").status, Status::Modified);
        assert_eq!(find(&files, "new.txt").status, Status::Added);
        git(dir, &["checkout", "--quiet", "main"]);
        let err = review.files(&Rev::Branch).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    }
}
