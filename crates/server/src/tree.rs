//! Turns the flat list of reviewed files into the folder tree that `GET /api/tree` returns.

use serde::Serialize;
use vugent_git::FileEntry;

/// A folder or a file of the reviewed folder.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    Folder {
        name: String,
        /// The path relative to the reviewed folder; empty for the reviewed folder itself.
        path: String,
        /// Folders first, then files, each sorted by name.
        children: Vec<Node>,
    },
    File {
        name: String,
        #[serde(flatten)]
        entry: FileEntry,
    },
}

impl Node {
    fn folder(name: &str, path: String) -> Self {
        Node::Folder {
            name: name.to_owned(),
            path,
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
        children.push(Node::File { name, entry });
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

#[cfg(test)]
mod tests {
    use super::*;
    use vugent_git::Status;

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
        }
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
}
