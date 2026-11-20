//! Syntax highlighting of whole files with tree-sitter-highlight, from ADR 0010.
//!
//! A file is highlighted whole, never a diff, so that tokens that open outside the changed lines, such as a block
//! comment, are colored right. The result is a list of lines, each a list of pieces of text with a class, which the
//! browser renders as elements rather than HTML.

use std::sync::LazyLock;

use serde::Serialize;
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

/// A piece of a highlighted line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Piece {
    pub text: String,
    /// What the text is, such as `keyword` or `string`, or `None` for plain text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<&'static str>,
}

/// The capture names that highlight queries use, each with the class it is shown with. tree-sitter-highlight matches
/// a capture such as `function.method.call` to the longest of these names that it starts with.
///
/// `none`, which Markdown uses to keep the content of a code block plain, is left out: a highlight it started would
/// open over the injected language's at the same position, and end in the wrong order.
const NAMES: [(&str, &str); 36] = [
    ("attribute", "attribute"),
    ("boolean", "constant"),
    ("comment", "comment"),
    ("constant", "constant"),
    ("constant.builtin", "constant"),
    ("constructor", "type"),
    ("embedded", "embedded"),
    ("escape", "escape"),
    ("function", "function"),
    ("function.builtin", "function"),
    ("function.macro", "macro"),
    ("keyword", "keyword"),
    ("label", "label"),
    ("module", "module"),
    ("namespace", "module"),
    ("number", "number"),
    ("operator", "operator"),
    ("property", "property"),
    ("punctuation", "punctuation"),
    ("punctuation.special", "special"),
    ("string", "string"),
    ("string.escape", "escape"),
    ("string.special", "special"),
    ("string.special.key", "property"),
    ("tag", "tag"),
    ("text.emphasis", "emphasis"),
    ("text.literal", "literal"),
    ("text.reference", "link"),
    ("text.strong", "strong"),
    ("text.title", "heading"),
    ("text.uri", "link"),
    ("type", "type"),
    ("type.builtin", "type"),
    ("variable", "variable"),
    ("variable.builtin", "builtin"),
    ("variable.parameter", "parameter"),
];

/// A language that vugent can highlight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Go,
    JavaScript,
    Json,
    Markdown,
    /// The inline parts of Markdown, such as emphasis and links, which the Markdown grammar parses separately.
    MarkdownInline,
    Python,
    Rust,
    Toml,
    Tsx,
    TypeScript,
    Yaml,
}

impl Language {
    /// Picks a language from a file's path, or `None` when there is no grammar for it.
    pub fn from_path(path: &str) -> Option<Self> {
        let name = path.rsplit('/').next().unwrap_or(path);
        if name == "Cargo.lock" {
            return Some(Self::Toml);
        }
        let (_, extension) = name.rsplit_once('.')?;
        Self::from_name(&extension.to_ascii_lowercase())
    }

    /// Picks a language from a name used by injections, such as a Markdown code block's ```` ```rust ````, or from a
    /// file extension.
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "go" | "golang" => Self::Go,
            "js" | "mjs" | "cjs" | "jsx" | "javascript" => Self::JavaScript,
            "json" => Self::Json,
            "md" | "markdown" => Self::Markdown,
            "markdown_inline" => Self::MarkdownInline,
            "py" | "pyi" | "python" => Self::Python,
            "rs" | "rust" => Self::Rust,
            "toml" => Self::Toml,
            "tsx" => Self::Tsx,
            "ts" | "mts" | "cts" | "typescript" => Self::TypeScript,
            "yaml" | "yml" => Self::Yaml,
            _ => return None,
        })
    }

    /// The language's highlighting, built the first time it is needed.
    fn config(self) -> &'static HighlightConfiguration {
        macro_rules! config {
            ($name:literal, $language:expr, $highlights:expr, $injections:expr, $locals:expr) => {{
                static CONFIG: LazyLock<HighlightConfiguration> = LazyLock::new(|| {
                    configure($name, $language.into(), &$highlights, $injections, $locals)
                });
                &CONFIG
            }};
        }
        match self {
            Self::Go => config!("go", tree_sitter_go::LANGUAGE, go_highlights(), "", ""),
            Self::JavaScript => config!(
                "javascript",
                tree_sitter_javascript::LANGUAGE,
                format!(
                    "{}\n{}",
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
                ),
                tree_sitter_javascript::INJECTIONS_QUERY,
                tree_sitter_javascript::LOCALS_QUERY
            ),
            Self::Json => config!(
                "json",
                tree_sitter_json::LANGUAGE,
                tree_sitter_json::HIGHLIGHTS_QUERY,
                "",
                ""
            ),
            Self::Markdown => config!(
                "markdown",
                tree_sitter_md::LANGUAGE,
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
                &markdown_injections(),
                ""
            ),
            Self::MarkdownInline => config!(
                "markdown_inline",
                tree_sitter_md::INLINE_LANGUAGE,
                tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                tree_sitter_md::INJECTION_QUERY_INLINE,
                ""
            ),
            Self::Python => config!(
                "python",
                tree_sitter_python::LANGUAGE,
                tree_sitter_python::HIGHLIGHTS_QUERY,
                "",
                ""
            ),
            Self::Rust => config!(
                "rust",
                tree_sitter_rust::LANGUAGE,
                tree_sitter_rust::HIGHLIGHTS_QUERY,
                tree_sitter_rust::INJECTIONS_QUERY,
                ""
            ),
            Self::Toml => config!(
                "toml",
                tree_sitter_toml_ng::LANGUAGE,
                tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
                "",
                ""
            ),
            // TypeScript's queries only add to JavaScript's, and come last so that they win.
            Self::Tsx => config!(
                "tsx",
                tree_sitter_typescript::LANGUAGE_TSX,
                format!(
                    "{}\n{}\n{}",
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                    tree_sitter_typescript::HIGHLIGHTS_QUERY
                ),
                tree_sitter_javascript::INJECTIONS_QUERY,
                tree_sitter_typescript::LOCALS_QUERY
            ),
            Self::TypeScript => config!(
                "typescript",
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
                format!(
                    "{}\n{}",
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_typescript::HIGHLIGHTS_QUERY
                ),
                tree_sitter_javascript::INJECTIONS_QUERY,
                tree_sitter_typescript::LOCALS_QUERY
            ),
            Self::Yaml => config!(
                "yaml",
                tree_sitter_yaml::LANGUAGE,
                tree_sitter_yaml::HIGHLIGHTS_QUERY,
                "",
                ""
            ),
        }
    }
}

/// Go's highlights, with the patterns for any identifier moved first. When several patterns capture a node, the last
/// one wins, and Go's query, unlike the others, was written for the first one winning.
fn go_highlights() -> String {
    const IDENTIFIERS: &str =
        "(type_identifier) @type\n(field_identifier) @property\n(identifier) @variable\n";
    let query = tree_sitter_go::HIGHLIGHTS_QUERY;
    assert!(query.contains(IDENTIFIERS), "Go's highlights changed");
    format!("{IDENTIFIERS}{}", query.replace(IDENTIFIERS, ""))
}

/// Markdown's injections, with the inline text's children included: they are the `*` of emphasis and the like, which
/// the inline grammar needs to see.
fn markdown_injections() -> String {
    const INLINE: &str = r#"(#set! injection.language "markdown_inline")"#;
    let query = tree_sitter_md::INJECTION_QUERY_BLOCK;
    assert!(query.contains(INLINE), "Markdown's injections changed");
    query.replace(
        INLINE,
        &format!("{INLINE}\n  (#set! injection.include-children)"),
    )
}

/// Builds a language's highlighting from its queries. The queries ship with the grammars and are tested, so a query
/// that does not compile is a bug in vugent.
fn configure(
    name: &str,
    language: tree_sitter::Language,
    highlights: &str,
    injections: &str,
    locals: &str,
) -> HighlightConfiguration {
    let mut config = HighlightConfiguration::new(language, name, highlights, injections, locals)
        .unwrap_or_else(|error| panic!("the {name} highlight queries do not compile: {error}"));
    config.configure(&NAMES.map(|(name, _)| name));
    config
}

/// Highlights the file at `path`, whose content is `source`. A file without a grammar, or one tree-sitter gives up on,
/// is plain text.
pub fn highlight_file(path: &str, source: &[u8]) -> Vec<Vec<Piece>> {
    Language::from_path(path)
        .and_then(|language| highlight(language, source))
        .unwrap_or_else(|| plain(source))
}

/// Splits `source` into lines of plain text, the way [`highlight`] does.
fn plain(source: &[u8]) -> Vec<Vec<Piece>> {
    let mut lines: Vec<Vec<Piece>> = source
        .split(|&byte| byte == b'\n')
        .map(|line| {
            let mut pieces = Vec::new();
            push(&mut pieces, line.strip_suffix(b"\r").unwrap_or(line), None);
            pieces
        })
        .collect();
    // A final line ending ends the last line rather than starting another.
    if source.ends_with(b"\n") || source.is_empty() {
        lines.pop();
    }
    lines
}

/// Highlights `source` as `language`, giving each of its lines, without line endings, as pieces. Returns `None` when
/// tree-sitter gives up, such as on a file it cannot parse in time.
pub fn highlight(language: Language, source: &[u8]) -> Option<Vec<Vec<Piece>>> {
    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(language.config(), source, None, |name| {
            Language::from_name(&name.to_ascii_lowercase()).map(Language::config)
        })
        .ok()?;

    let mut lines = vec![Vec::new()];
    // The highlights open at this point, innermost last.
    let mut open: Vec<Highlight> = Vec::new();
    for event in events {
        match event.ok()? {
            HighlightEvent::HighlightStart(highlight) => open.push(highlight),
            HighlightEvent::HighlightEnd => {
                open.pop();
            }
            HighlightEvent::Source { start, end } => {
                let class = open
                    .last()
                    .map(|highlight| NAMES[highlight.0].1)
                    .filter(|&class| class != "none");
                // A token that spans lines, such as a block comment, is split at each line end.
                let mut pieces = source[start..end].split(|&byte| byte == b'\n');
                if let Some(first) = pieces.next() {
                    push(lines.last_mut()?, first, class);
                }
                for rest in pieces {
                    lines.push(Vec::new());
                    push(lines.last_mut()?, rest, class);
                }
            }
        }
    }
    // A final line ending ends the last line rather than starting another, the way git counts lines.
    if lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    for line in &mut lines {
        // git's diffs show lines without a carriage return before the line feed.
        if let Some(last) = line.last_mut() {
            if last.text.ends_with('\r') {
                last.text.pop();
            }
            if last.text.is_empty() {
                line.pop();
            }
        }
    }
    Some(lines)
}

/// Adds `text` to `line`, joining it to the last piece when that has the same class.
fn push(line: &mut Vec<Piece>, text: &[u8], class: Option<&'static str>) {
    if text.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(text);
    match line.last_mut() {
        Some(last) if last.class == class => last.text.push_str(&text),
        _ => line.push(Piece {
            text: text.into_owned(),
            class,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each line as `class:text` pieces joined by `|`, with `-` for plain text.
    fn render(lines: &[Vec<Piece>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|piece| format!("{}:{}", piece.class.unwrap_or("-"), piece.text))
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect()
    }

    #[test]
    fn every_query_compiles() {
        for language in [
            Language::Go,
            Language::JavaScript,
            Language::Json,
            Language::Markdown,
            Language::MarkdownInline,
            Language::Python,
            Language::Rust,
            Language::Toml,
            Language::Tsx,
            Language::TypeScript,
            Language::Yaml,
        ] {
            language.config();
        }
    }

    #[test]
    fn picks_languages_from_paths() {
        assert_eq!(Language::from_path("src/main.rs"), Some(Language::Rust));
        assert_eq!(Language::from_path("a/b/App.TSX"), Some(Language::Tsx));
        assert_eq!(Language::from_path("Cargo.lock"), Some(Language::Toml));
        assert_eq!(Language::from_path("README.md"), Some(Language::Markdown));
        assert_eq!(Language::from_path("Makefile"), None);
        assert_eq!(Language::from_path("notes.txt"), None);
        assert_eq!(Language::from_path(".go/x"), None);
    }

    #[test]
    fn files_without_a_grammar_are_plain_text() {
        let lines = highlight_file("notes.txt", b"one\r\n\nthree");

        assert_eq!(render(&lines), ["-:one", "", "-:three"]);
        assert!(highlight_file("notes.txt", b"").is_empty());
        assert_eq!(highlight_file("notes.txt", b"\n").len(), 1);
    }

    #[test]
    fn highlights_every_line() {
        let lines = highlight(Language::Go, b"package main\n\nfunc main() {}\n").unwrap();

        assert_eq!(lines.len(), 3);
        assert_eq!(render(&lines)[0], "keyword:package|-: main");
        assert!(lines[1].is_empty());
        assert!(render(&lines)[2].starts_with("keyword:func|-: |function:main"));
    }

    #[test]
    fn splits_tokens_at_line_ends() {
        let lines = highlight(Language::Rust, b"/* one\ntwo */ fn x() {}\r\n").unwrap();

        assert_eq!(render(&lines)[0], "comment:/* one");
        assert!(render(&lines)[1].starts_with("comment:two */|-: |keyword:fn"));
        assert!(!lines[1].last().unwrap().text.ends_with('\r'));
    }

    #[test]
    fn keeps_a_last_line_without_a_line_ending() {
        let lines = highlight(Language::Json, b"{\n  \"a\": 1\n}").unwrap();

        assert_eq!(lines.len(), 3);
        assert_eq!(render(&lines)[1], "-:  |string:\"a\"|-:: |number:1");
        assert_eq!(render(&lines)[2], "-:}");
    }

    #[test]
    fn markdown_highlights_inline_text_and_code_blocks() {
        let source = b"# Title\n\nSome *text*.\n\n```rust\nfn main() {}\n```\n";

        let lines = highlight(Language::Markdown, source).unwrap();

        assert!(render(&lines)[0].contains("heading:"));
        assert!(render(&lines)[2].contains("emphasis:"));
        assert!(render(&lines)[5].contains("keyword:fn"));
    }

    #[test]
    fn typescript_inherits_javascript() {
        let lines = highlight(Language::TypeScript, b"const x: number = 1;\n").unwrap();

        assert!(render(&lines)[0].contains("keyword:const"));
        assert!(render(&lines)[0].contains("type:number"));
    }
}
