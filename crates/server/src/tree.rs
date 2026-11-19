//! Turns the flat list of reviewed files into the folder tree that `GET /api/tree` returns, colored as ADR 0004
//! describes.

use serde::Serialize;
use vugent_git::{FileEntry, Status};

/// A folder or a file of the reviewed folder.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    Folder {
        name: String,
        /// The path relative to the reviewed folder; empty for the reviewed folder itself.
        path: String,
        /// Lines added in every file inside the folder, at any depth.
        added: u64,
        /// Lines removed in every file inside the folder, at any depth.
        removed: u64,
        #[serde(flatten)]
        color: Color,
        /// Folders first, then files, each sorted by name.
        children: Vec<Node>,
    },
    File {
        name: String,
        #[serde(flatten)]
        entry: FileEntry,
        #[serde(flatten)]
        color: Color,
    },
}

/// How a box is colored: its hue says what kind of change happened, its shade how much of the box changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Color {
    pub hue: Hue,
    /// Absent when the hue is `none`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shade: Option<Shade>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Hue {
    /// Nothing was added or removed.
    #[default]
    None,
    /// Lines were only added.
    Green,
    /// Lines were only removed.
    Red,
    /// Lines were both added and removed.
    Orange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Shade {
    /// Less than half of the lines changed.
    Light,
    /// At least half, but not all, of the lines changed.
    Medium,
    /// Every line changed.
    Dark,
}

/// The four numbers a color is computed from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Counts {
    added: u64,
    removed: u64,
    /// Lines in `HEAD`.
    old: u64,
    /// Lines in the working tree.
    new: u64,
}

impl Counts {
    fn of(entry: &FileEntry) -> Self {
        let (added, removed) = if entry.binary || entry.submodule {
            // No line counts: the whole thing counts as one line.
            match entry.status {
                Status::Added => (1, 0),
                Status::Deleted => (0, 1),
                Status::Modified | Status::Renamed => (1, 1),
                Status::Unchanged => (0, 0),
            }
        } else {
            (u64::from(entry.added), u64::from(entry.removed))
        };
        let new = u64::from(entry.lines);
        Self {
            added,
            removed,
            // Saturating, in case the file changed between running git and counting its lines.
            old: (new + removed).saturating_sub(added),
            new,
        }
    }

    fn color(self) -> Color {
        let (hue, part, whole) = match (self.added > 0, self.removed > 0) {
            (false, false) => return Color::default(),
            (true, false) => (Hue::Green, self.added, self.new),
            (false, true) => (Hue::Red, self.removed, self.old),
            (true, true) => (
                Hue::Orange,
                self.added.max(self.removed),
                self.old.max(self.new),
            ),
        };
        let shade = if part >= whole {
            Shade::Dark
        } else if part * 2 >= whole {
            Shade::Medium
        } else {
            Shade::Light
        };
        Color {
            hue,
            shade: Some(shade),
        }
    }
}

impl std::ops::AddAssign for Counts {
    fn add_assign(&mut self, other: Self) {
        self.added += other.added;
        self.removed += other.removed;
        self.old += other.old;
        self.new += other.new;
    }
}

impl Node {
    fn folder(name: &str, path: String) -> Self {
        Node::Folder {
            name: name.to_owned(),
            path,
            added: 0,
            removed: 0,
            color: Color::default(),
            children: Vec::new(),
        }
    }
}

/// Builds the tree of `files`, whose root is named `root_name`.
pub fn build(root_name: &str, files: Vec<FileEntry>) -> Node {
    let mut root = Node::folder(root_name, String::new());
    for entry in files {
        insert(&mut root, entry);
    }
    sort(&mut root);
    paint(&mut root);
    root
}

fn insert(root: &mut Node, entry: FileEntry) {
    let mut node = root;
    let mut parts: Vec<&str> = entry.path.split('/').collect();
    let name = parts.pop().unwrap_or_default().to_owned();
    let mut path = String::new();
    for part in parts {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(part);
        let Node::Folder { children, .. } = node else {
            unreachable!("only folders are descended into")
        };
        let index = match children
            .iter()
            .position(|child| matches!(child, Node::Folder { name, .. } if name == part))
        {
            Some(index) => index,
            None => {
                children.push(Node::folder(part, path.clone()));
                children.len() - 1
            }
        };
        node = &mut children[index];
    }
    if let Node::Folder { children, .. } = node {
        children.push(Node::File {
            name,
            entry,
            color: Color::default(),
        });
    }
}

fn sort(node: &mut Node) {
    if let Node::Folder { children, .. } = node {
        children.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
        children.iter_mut().for_each(sort);
    }
}

fn sort_key(node: &Node) -> (bool, &str) {
    match node {
        Node::Folder { name, .. } => (false, name),
        Node::File { name, .. } => (true, name),
    }
}

/// Colors `node` and everything inside it, and returns its counts.
fn paint(node: &mut Node) -> Counts {
    match node {
        Node::File { entry, color, .. } => {
            let counts = Counts::of(entry);
            *color = counts.color();
            counts
        }
        Node::Folder {
            added,
            removed,
            color,
            children,
            ..
        } => {
            let mut counts = Counts::default();
            for child in children {
                counts += paint(child);
            }
            (*added, *removed) = (counts.added, counts.removed);
            *color = counts.color();
            counts
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            path: path.to_owned(),
            status: Status::Modified,
            added: 1,
            removed: 2,
            binary: false,
            old_path: None,
            submodule: false,
            untracked: false,
            lines: 3,
        }
    }

    /// A file with the given counts, `new` being its line count in the working tree.
    fn counted(path: &str, status: Status, added: u32, removed: u32, new: u32) -> FileEntry {
        FileEntry {
            status,
            added,
            removed,
            lines: new,
            ..entry(path)
        }
    }

    fn color_of(entry: FileEntry) -> Value {
        let tree = serde_json::to_value(build("repo", vec![entry])).unwrap();
        let file = &tree["children"][0];
        json!([file["hue"], file.get("shade")])
    }

    fn hue_and_shade(hue: &str, shade: &str) -> Value {
        json!([hue, shade])
    }

    #[test]
    fn nests_files_in_folders_sorted_folders_first() {
        let tree = build(
            "repo",
            vec![
                entry("b.txt"),
                entry("src/x/y.rs"),
                entry("a.txt"),
                entry("src/m.rs"),
            ],
        );

        let json = serde_json::to_value(&tree).unwrap();
        assert_eq!(json["kind"], "folder");
        assert_eq!(json["name"], "repo");
        assert_eq!(json["path"], "");
        let names: Vec<_> = json["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|child| child["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["src", "a.txt", "b.txt"]);
        let src = &json["children"][0];
        assert_eq!(src["children"][0]["path"], "src/x");
        assert_eq!(src["children"][0]["children"][0]["path"], "src/x/y.rs");
        let file = &src["children"][1];
        assert_eq!(file["kind"], "file");
        assert_eq!(file["name"], "m.rs");
        assert_eq!(file["status"], "modified");
        assert_eq!(file["added"], 1);
        assert_eq!(file["removed"], 2);
        assert!(file.get("oldPath").is_none());
    }

    #[test]
    fn hue_follows_what_was_added_and_removed() {
        assert_eq!(
            color_of(counted("a", Status::Unchanged, 0, 0, 5)),
            json!(["none", null])
        );
        assert_eq!(
            color_of(counted("a", Status::Modified, 1, 0, 10)),
            hue_and_shade("green", "light")
        );
        assert_eq!(
            color_of(counted("a", Status::Modified, 0, 1, 9)),
            hue_and_shade("red", "light")
        );
        assert_eq!(
            color_of(counted("a", Status::Modified, 1, 1, 10)),
            hue_and_shade("orange", "light")
        );
    }

    #[test]
    fn shade_follows_the_share_of_lines_changed() {
        // Green: added ÷ new.
        assert_eq!(
            color_of(counted("a", Status::Modified, 4, 0, 10)),
            hue_and_shade("green", "light")
        );
        assert_eq!(
            color_of(counted("a", Status::Modified, 5, 0, 10)),
            hue_and_shade("green", "medium")
        );
        assert_eq!(
            color_of(counted("a", Status::Added, 10, 0, 10)),
            hue_and_shade("green", "dark")
        );
        // Red: removed ÷ old, where old = 6 - 0 + 4 = 10.
        assert_eq!(
            color_of(counted("a", Status::Modified, 0, 4, 6)),
            hue_and_shade("red", "light")
        );
        assert_eq!(
            color_of(counted("a", Status::Modified, 0, 5, 5)),
            hue_and_shade("red", "medium")
        );
        assert_eq!(
            color_of(counted("a", Status::Deleted, 0, 10, 0)),
            hue_and_shade("red", "dark")
        );
        // Orange: max(added, removed) ÷ max(old, new).
        assert_eq!(
            color_of(counted("a", Status::Modified, 10, 10, 100)),
            hue_and_shade("orange", "light")
        );
        assert_eq!(
            color_of(counted("a", Status::Modified, 3, 6, 7)),
            hue_and_shade("orange", "medium")
        );
        assert_eq!(
            color_of(counted("a", Status::Modified, 8, 10, 8)),
            hue_and_shade("orange", "dark")
        );
    }

    #[test]
    fn binary_files_and_submodules_count_as_one_line() {
        let binary = |status| FileEntry {
            binary: true,
            ..counted("a", status, 0, 0, u32::from(status != Status::Deleted))
        };
        assert_eq!(
            color_of(binary(Status::Added)),
            hue_and_shade("green", "dark")
        );
        assert_eq!(
            color_of(binary(Status::Deleted)),
            hue_and_shade("red", "dark")
        );
        assert_eq!(
            color_of(binary(Status::Modified)),
            hue_and_shade("orange", "dark")
        );
        assert_eq!(color_of(binary(Status::Unchanged)), json!(["none", null]));
        let submodule = FileEntry {
            submodule: true,
            ..counted("a", Status::Modified, 0, 0, 1)
        };
        assert_eq!(color_of(submodule), hue_and_shade("orange", "dark"));
    }

    #[test]
    fn a_pure_rename_is_unchanged() {
        let renamed = FileEntry {
            old_path: Some("b".to_owned()),
            ..counted("a", Status::Renamed, 0, 0, 5)
        };

        assert_eq!(color_of(renamed), json!(["none", null]));
    }

    #[test]
    fn folders_sum_every_file_inside_them() {
        let tree = build(
            "repo",
            vec![
                counted("new/a", Status::Added, 3, 0, 3),
                counted("new/deep/b", Status::Added, 2, 0, 2),
                counted("mixed/a", Status::Added, 3, 0, 3),
                counted("mixed/b", Status::Deleted, 0, 4, 0),
                counted("big/small", Status::Modified, 1, 0, 2),
                counted("big/x", Status::Unchanged, 0, 0, 100),
            ],
        );

        let json = serde_json::to_value(&tree).unwrap();
        let folder = |index: usize| {
            let folder = &json["children"][index];
            json!([
                folder["name"],
                folder["added"],
                folder["removed"],
                folder["hue"],
                folder["shade"]
            ])
        };
        assert_eq!(folder(0), json!(["big", 1, 0, "green", "light"]));
        assert_eq!(folder(1), json!(["mixed", 3, 4, "orange", "dark"]));
        assert_eq!(folder(2), json!(["new", 5, 0, "green", "dark"]));
        assert_eq!(
            json!([json["added"], json["removed"], json["hue"], json["shade"]]),
            json!([9, 4, "orange", "light"])
        );
    }
}
