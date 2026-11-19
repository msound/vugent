//! Which paths git ignores, from every `.gitignore` in the working tree, `info/exclude` and the user's global
//! excludes file, matched with the `ignore` crate.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The gitignore rules of a working tree, and the folders of the reviewed folder that are not ignored.
pub(crate) struct Ignores {
    /// The top of the working tree.
    root: PathBuf,
    /// Each folder's `.gitignore`, by folder.
    files: HashMap<PathBuf, Gitignore>,
    /// `info/exclude`, which ranks below every `.gitignore`.
    exclude: Gitignore,
    /// The user's global excludes file, which ranks lowest.
    global: Gitignore,
    /// Nested repositories and submodules, which git shows as a single entry; nothing inside them is reviewed.
    nested: HashSet<PathBuf>,
}

impl Ignores {
    /// Reads the rules that apply to `dir`, a folder of the working tree whose top is `root`, and lists every folder
    /// under `dir` that is not ignored, `dir` included. `common_dir` is the git directory that holds `info/exclude`.
    pub(crate) fn load(root: &Path, common_dir: &Path, dir: &Path) -> (Self, Vec<PathBuf>) {
        let mut exclude = GitignoreBuilder::new(root);
        exclude.add(common_dir.join("info/exclude"));
        let mut ignores = Self {
            root: root.to_owned(),
            files: HashMap::new(),
            exclude: exclude.build().unwrap_or_else(|_| Gitignore::empty()),
            global: GitignoreBuilder::new(root).build_global().0,
            nested: HashSet::new(),
        };
        // The .gitignore files above the reviewed folder apply to it too.
        let above: Vec<&Path> = dir
            .ancestors()
            .skip(1)
            .take_while(|folder| folder.starts_with(root))
            .collect();
        for folder in above.into_iter().rev() {
            ignores.read(folder);
        }

        let mut folders = Vec::new();
        let mut pending = vec![dir.to_owned()];
        while let Some(folder) = pending.pop() {
            if folder != root && folder.join(".git").exists() {
                ignores.nested.insert(folder);
                continue;
            }
            ignores.read(&folder);
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let is_folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
                let path = entry.path();
                if is_folder && entry.file_name() != ".git" && !ignores.matched(&path, true) {
                    pending.push(path);
                }
            }
            folders.push(folder);
        }
        folders.sort();
        (ignores, folders)
    }

    fn read(&mut self, folder: &Path) {
        let file = folder.join(".gitignore");
        if file.is_file() {
            let (gitignore, _) = Gitignore::new(&file);
            self.files.insert(folder.to_owned(), gitignore);
        }
    }

    /// Whether git ignores `path`, an absolute path, or anything inside `.git` or a nested repository. A folder
    /// that is ignored ignores everything inside it, as in git.
    pub(crate) fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let mut current = self.root.clone();
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                continue;
            };
            current.push(name);
            if name == ".git" || self.nested.contains(&current) {
                return true;
            }
            let last = components.peek().is_none();
            if self.matched(&current, !last || is_dir) {
                return true;
            }
        }
        false
    }

    /// Whether `path` itself matches an ignore rule, the deepest `.gitignore` deciding first.
    fn matched(&self, path: &Path, is_dir: bool) -> bool {
        let folders = path
            .ancestors()
            .skip(1)
            .take_while(|folder| folder.starts_with(&self.root));
        let matches = folders
            .filter_map(|folder| self.files.get(folder))
            .chain([&self.exclude, &self.global])
            .map(|gitignore| gitignore.matched(path, is_dir));
        for found in matches {
            match found {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, path: &str, content: &str) {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// A working tree with ignore rules at several levels.
    fn tree() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".git/info")).unwrap();
        write(root, ".git/info/exclude", "*.local\n");
        write(root, ".gitignore", "target/\n*.log\n");
        write(root, "web/.gitignore", "node_modules\n!keep.log\n");
        write(root, "web/node_modules/x/index.js", "");
        write(root, "web/src/app.ts", "");
        write(root, "target/debug/vugent", "");
        write(root, "src/lib.rs", "");
        write(root, "vendor/nested/.git/HEAD", "");
        write(root, "vendor/nested/file.txt", "");
        tmp
    }

    fn load(root: &Path, dir: &Path) -> (Ignores, Vec<PathBuf>) {
        Ignores::load(root, &root.join(".git"), dir)
    }

    #[test]
    fn lists_folders_that_are_not_ignored() {
        let tmp = tree();
        let root = tmp.path();

        let (_, folders) = load(root, root);

        let names: Vec<_> = folders
            .iter()
            .map(|folder| folder.strip_prefix(root).unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, ["", "src", "vendor", "web", "web/src"]);
    }

    #[test]
    fn matches_rules_at_every_level() {
        let tmp = tree();
        let root = tmp.path();
        let (ignores, _) = load(root, root);
        let ignored = |path: &str| ignores.is_ignored(&root.join(path), false);

        assert!(ignored("target/debug/vugent"));
        assert!(ignored("debug.log"));
        assert!(ignored("web/node_modules/x/index.js"));
        assert!(ignored("notes.local"));
        assert!(ignored(".git/index"));
        assert!(ignored("vendor/nested/file.txt"));
        assert!(!ignored("web/keep.log"));
        assert!(!ignored("web/src/app.ts"));
        assert!(!ignored("src/lib.rs"));
        assert!(!ignored(".gitignore"));
    }

    #[test]
    fn a_subfolder_uses_the_rules_above_it() {
        let tmp = tree();
        let root = tmp.path();
        write(root, "web/src/debug.log", "");

        let (ignores, folders) = load(root, &root.join("web"));

        assert_eq!(folders, [root.join("web"), root.join("web/src")]);
        assert!(ignores.is_ignored(&root.join("web/src/debug.log"), false));
    }
}
