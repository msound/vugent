# Rust core with WASM language plugins and a Svelte frontend

## Context and Problem Statement

Vugent is invoked as `vugent .`, starts a local HTTP server, and shows a browser UI with a block diagram of the
project's modules, colored by how they changed, with drill-down into syntax-highlighted diffs (#1). Go is supported
first, but the project is expected to grow to a long tail of languages and formats, including declarative ones such
as Helm charts and Terraform. Analysis must work on any git revision, not only the checked-out working tree.

Which language, libraries and distribution mechanism should vugent use, and how should language support be
structured so that adding languages, including by third parties, does not depend on the core team?

Decision drivers:

* Single-binary install on developer laptops, with no language runtime to set up.
* Language support must scale to many languages and formats, ideally contributed by others.
* Analysis operates on file contents at arbitrary git revisions.
* Live updates in phase 2 (#6) require incremental re-analysis and server-to-browser push.
* Large monorepos must remain usable.

## Considered Options

Core language:

* Go
* Rust
* TypeScript (Node/Bun)

Language support:

* Built-in analyzers only (native parsers per language)
* Built-in analyzers using tree-sitter
* Tree-sitter plus WASM component plugins

Frontend:

* No build step (vendored ES modules, htmx/Lit)
* Vite + Svelte 5
* Vite + React
* Rust compiled to WASM (Leptos/Dioxus)

Git access:

* `git` CLI
* Pure library (go-git / gix)
* libgit2

## Decision Outcome

Chosen option: **Rust core, tree-sitter based analysis with WASM component plugins, Vite + Svelte 5 frontend,
and the `git` CLI**, because it makes the long tail of languages an ecosystem concern rather than a core team
concern: tree-sitter grammars are native in Rust, wasmtime is the most capable WASM component host, and the WASM
sandbox makes running third-party analyzers on developer machines safe. Multiple competing implementations for the
same language are possible, giving users a choice.

The decision in detail:

| Area | Decision |
|---|---|
| Core language | Rust, organized as a cargo workspace (cli, server, core, git, analysis, watch, highlight, plugin host, first-party language crates). |
| CLI and server | clap; axum on tokio; Server-Sent Events for pushing live updates to the browser. |
| Distribution | cargo-dist: GitHub Releases, Homebrew tap, shell/PowerShell installers. Also published to crates.io; the published crate includes the prebuilt frontend so `cargo install vugent` does not need Node. |
| Frontend | Vite + Svelte 5 + TypeScript. Built in CI, embedded into the binary with rust-embed. Built assets are not committed. In development, the Vite dev server proxies `/api` to the Rust server. |
| Block diagram | Cytoscape.js (canvas rendering, compound nodes, scales to large graphs) with elkjs layout running in a Web Worker. Layout happens in the browser; the server sends graph data only. |
| Diffs and highlighting | Diffs computed server-side via git. Syntax highlighting server-side with tree-sitter-highlight, emitting HTML with CSS classes; themes (light/dark) are handled in CSS. Languages without highlight queries fall back to plain text. |
| Source analysis | tree-sitter grammars and queries. Analyzers are given file bytes at a revision through host functions and never read the working tree directly, which makes analysis of arbitrary revisions the default. |
| Language extensibility | WASM component plugins hosted with wasmtime, defined by a WIT interface. First-party analyzers (Go first) are built against the same interface. In v1 plugins are data-only: they return modules, edges and metadata, and the core renders all UI. Details are deferred to the plugin architecture ADR (#8). |
| Git access | The `git` CLI: `cat-file --batch` for reading blobs at a revision, `diff --numstat -M` for changed files and line counts, `merge-base` and `rev-parse` for revision resolution. gix may be adopted later for in-process object reads. |
| External requirements | `git` on `PATH`. Nothing else. |

### Consequences

* Good, because tree-sitter grammars are plain crate dependencies; no cgo-style toolchain trade-off exists.
* Good, because the same grammars power both import analysis and syntax highlighting, so every supported
  language also gets highlighted diffs.
* Good, because the WASM sandbox restricts plugins to read-only host capabilities, so third-party analyzers are
  safe to run and cannot bypass the any-revision model.
* Good, because Rust offers incremental computation frameworks (e.g. salsa) suited to live re-analysis (#6).
* Good, because using the `git` CLI matches the user's git behavior exactly (worktrees, config, rename detection,
  partial clones).
* Good, because the frontend toolchain is only needed to work on the frontend; installing vugent never needs Node.
* Bad, because the Go analyzer cannot use `go/parser` and `go/build`; build constraints, `go.work`, `replace`
  directives and `vendor/` must be reimplemented.
* Bad, because Go-ecosystem semantics (e.g. rendering Helm templates, evaluating Terraform exactly) are not
  available natively. Structural graphs (subcharts, module blocks) are fine; deeper analysis would require a
  plugin compiled from Go to WASM or an optional external tool.
* Bad, because the plugin interface becomes a public contract; it must be versioned and kept unstable until it
  has been proven on several languages.
* Bad, because Rust has slower iteration and a higher barrier for contributors than Go; mitigated by making
  simple language support possible with grammars, queries and configuration only.
* Bad, because the frontend adds a Node-based toolchain to development and CI.
