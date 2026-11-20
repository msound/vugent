# Unified or side-by-side diffs of whole files, highlighted per side and cached by the browser

## Context and Problem Statement

The diff panel (ADR 0003) shows one unified pane with git's default three lines of context around each change, as
plain text. Reviewers often want the old and new code side by side, want to see more of the file than the lines
around a change, and expect syntax highlighting, which ADR 0001 chose to do on the server with
tree-sitter-highlight but which is not built yet.

How does the user switch layouts, how do they see the whole file, how is a diff highlighted when a unified diff
mixes old and new lines and cannot be parsed as code, and how is highlighting cached? Issue #32 describes the
problem.

## Considered Options

Side-by-side layout:

* Built in the browser from the hunks `/api/diff` already returns
* A separate server response for each layout

Seeing the whole file:

* A toggle that asks git for unlimited context (`-U<huge>`)
* Hunks as today, with buttons that fetch more lines from the server
* The whole file always sent, with unchanged stretches folded in the browser

Highlighting:

* Highlighting the diff text
* Highlighting the old and new files whole, and giving each diff line the colors of its line on its own side

The form of highlighted lines:

* HTML from tree-sitter-highlight's renderer, inserted with `{@html}`
* Structured pieces of text and class, rendered with Svelte elements

Caching highlighting:

* An in-memory cache on the server, keyed by blob ID
* A cache on disk
* The browser's HTTP cache, with highlighting served at URLs named by blob ID

## Decision Outcome

Chosen options: **side-by-side built in the browser**, **the whole file always sent and folded in the browser**,
**the old and new files highlighted whole** and sent as **structured pieces**, and **the browser's HTTP cache** with
URLs named by blob ID.

* Every line in a hunk already carries its old and new line numbers, so the browser has all it needs to lay out
  either view, and switching layouts needs no request.
* Sending the whole file keeps every decision about context in the browser, so unfolding is instant. Highlighting
  needs the whole of both files anyway: a block comment or a multi-line string that opens outside the hunk would
  otherwise be colored wrongly.
* Highlighting each side whole and looking colors up by line number works the same in both layouts: the layout only
  decides where a line goes, never how it is colored.
* Structured pieces keep raw HTML out of the page. Since vugent can listen beyond localhost (ADR 0009), file content
  is never inserted with `{@html}`, even when escaped by the server.
* Content named by its blob ID never changes, so the browser can keep it for good and serve it without a request.
  After a live update (ADR 0008), only the side that changed has a new ID, so the base side is not fetched or
  highlighted again. A server-side cache would only help several tabs looking at the same review, and a cache on
  disk is left to the ADR on caching and local storage (#7).

### Layout

| | Decision |
|---|---|
| Unified | As today: old and new line numbers, a sign, and the line. |
| Side-by-side | Old lines on the left, new lines on the right, each with its line number. An unchanged line appears on both sides. A run of removed lines followed by a run of added lines is paired row by row; where one run is longer, the other side shows blank cells. A line only added or only removed sits opposite a blank cell. |
| Switching | A toggle in the diff panel's header, and the `S` key. |
| Default | Side-by-side when the window is at least 1200 CSS pixels wide, unified otherwise. Once the user picks a layout, it is remembered in `localStorage` and used from then on. |

Highlighting the words that changed within a pair of lines is left for later.

### The whole file

* `/api/diff` returns every line of the file instead of hunks, each marked added, removed or unchanged, with its
  old and new line numbers. A file larger than 1 MiB on either side gets git's hunks as today, and is not
  highlighted.
* The browser shows unchanged lines up to three lines away from a change, and folds the rest into a row such as
  "⋯ 42 unchanged lines", which unfolds when clicked. A stretch of four lines or fewer is never folded, since the
  row would take as much room as the lines.
* A toggle in the header and the `A` key unfold all lines, or fold them again. The choice is not remembered: every
  diff opens folded.
* Folding is kept when the diff is fetched again after a live update, for the stretches that still exist.

### Highlighting

* A new `highlight` crate picks a grammar from the file's name and highlights a whole file with
  tree-sitter-highlight. The first languages are Go, Rust, TypeScript, JavaScript, Python, JSON, YAML, TOML and
  Markdown. Anything else is plain text.
* The old and new files are highlighted separately. A removed line takes the colors of its line in the old file; an
  added or unchanged line takes those of its line in the new file.
* Each line is a list of pieces, each a piece of text with a highlight class such as `keyword` or `string`, or
  none. A token that spans lines, such as a block comment, is split at each line end and its class starts again on
  the next line.
* Colors are CSS custom properties for the light and dark themes, chosen to stay readable on the added and removed
  backgrounds as well as the plain one.

### Requests and caching

* `/api/diff` stays `no-store`, since the diff of the working tree, the index or the branch changes all the time,
  and every repository is served from the same `localhost:<port>` origin. It gives each side's blob ID.
* A commit's diff (`rev=<full SHA>`, parent → commit) never changes, so it is served as
  `private, max-age=31536000, immutable`, like `/api/commit`.
* `GET /api/highlight/<blob-oid>?path=<path>&v=<version>` returns a file's highlighted lines as
  `private, max-age=31536000, immutable`. The path picks the grammar and, for the working tree, where to read the
  file.
* A file in the working tree has no blob in git, so its ID is computed by hashing its content the way
  `git hash-object` does, in the repository's hash format, without writing it. Before answering, the server checks
  that the file still has that ID, and answers 404 if it changed in the meantime; the browser then fetches the diff
  again.
* `v` is vugent's version, so that an upgrade, which may change grammars or the response's shape, is never served
  old answers. The diff of a commit carries it too.
* There is no cache on the server.

### Consequences

* Good, because switching layouts, unfolding and refetching after a live update are fast, and the base side of a
  diff is highlighted only once.
* Good, because highlighting is correct across hunk boundaries, and identical in both layouts.
* Good, because no file content reaches the page as raw HTML.
* Bad, because `/api/diff` sends whole files, which is more data than hunks for large files with small changes.
* Bad, because git's function context in hunk headers (`@@ … @@ func main()`) is lost for whole files; it could
  come back from tree-sitter on fold rows later.
* Bad, because every grammar makes the binary larger, and languages outside the first set stay plain text.
* Bad, because a diff and its highlighting are two requests, and a working-tree file is read and hashed twice.
