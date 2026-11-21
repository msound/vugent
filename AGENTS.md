# Vugent

## Overview

Vugent (a portmanteau of view + agent) is a tool to help users stay on top of what their agent is doing, as well as
help users review code.

## Issue tracking

This project is hosted on Github, and issue tracking is done on Github. You can access the issues via `gh` CLI
command. Here are some popular commands:

List issues: `gh issue list`
View issue: `gh issue view <n>`
Create issue: `gh issue create --title "some title" --label bug --body-file - <<EOF
long
multi-line
description
goes
here
EOF

When creating issues, use one of the labels: enhancement, bug, documentation (such as an issue to write an ADR).

Closing an issue: This one is tricky. Instead of running `gh issue close <n>`, you will be adding `Closes: #n` to the
footer of the git commit message. When git-pushed, Github will automatically close the issue. Once pushed, there is
no need to run `gh issue list` to confirm, just trust Github to do its job.

## Conventional commits

When commiting to git, we use "conventional commits" for writing commit messages. Avoid writing a scope as part of the
commit message title. Here is an example:

```
feat: reticulate all spines

Longer description about reticulating spines,
what code was changed, and why.

Closes: #1
```

Allowed types: feat, fix, chore, ci, docs, style, refactor, perf, test.

## Working on this project

Look at list of issues, pick one and confirm with user if you can work on that issue next.

Come up with a plan for the issue, confirm with the user, then begin coding.

When done, ask user to review before committing. Always push after committing, you don't need to ask user for
permission. We would like to see issues close in Github, and that won't happen until we push.

It is ok to tackle one issue across multiple commits to keep the code review diff manageable. In that case,
first make a plan of commits you are going to make and get user approval. At every commit, linting and all
tests must pass. Use `Refs: #n` in the footer of intermediate commits, and `Closes: #n` only on the final one.

## Notes

Releases are made with cocogitto (`cog bump --auto`), configured in `cog.toml`. Its `chore(version): vX.Y.Z`
release commit is the one allowed exception to the no-scope rule; never add scopes to other commits.


## Architecture decisions

Decisions are recorded in docs/decisions/ using the MADR template (docs/decisions/0000-template.md).
Below is a one sentence summary of each. Read the full ADR when working in an area it covers, or before
proposing something that contradicts it. When a new decision is made, add an ADR with the next number and
a summary line here.

* [0000](docs/decisions/0000-template.md): A minimal MADR (Markdown Architecture Decision Record) template to use.
* [0001](docs/decisions/0001-tech-stack.md): Rust core (axum, cargo-dist) with tree-sitter analysis via WASM component plugins, a Vite + Svelte 5 frontend with elkjs (diagram library superseded by 0003), server-side highlighting, and the `git` CLI as the only external requirement.
* [0002](docs/decisions/0002-review-uncommitted-changes.md): v0.1.0 reviews all uncommitted changes, untracked files included, against HEAD; a review is modeled as a (base, target) pair so that branch and single-commit modes can be added later.
* [0003](docs/decisions/0003-semantic-zoom-canvas.md): The UI is a Svelte Flow canvas (replacing Cytoscape.js) with one elkjs layout of the folder tree, where folders open as they grow on screen and a file's diff opens in a full-screen panel closed with Esc.
* [0004](docs/decisions/0004-change-colors.md): Files and folders are green, red or orange by whether lines were added, removed or both, in three shades (under 50%, at least 50%, 100%) of lines changed relative to size, with folders summing the line counts of every file inside them.
* [0005](docs/decisions/0005-branch-and-commit-review.md): A sidebar lists the branch (merge-base → working tree) with uncommitted changes, staged changes (only when present) and each first-parent commit (parent → commit, tree as of that commit) indented under it, navigated with J/K.
* [0006](docs/decisions/0006-staged-and-unstaged-changes.md): While something is staged, "Uncommitted changes" is replaced by "Unstaged changes" (index → working tree, untracked files included) and then "Staged changes" (HEAD → index), newest first, and the first row under the branch opens; this supersedes the staged-changes part of 0005.
* [0007](docs/decisions/0007-review-header-and-detail-page.md): Every review gets a two-line header across the top of the canvas (a commit's subject and metadata, the branch's base, or a file count, with line counts), and scrolling up into it or pressing Enter opens a full-screen detail page with the full message, trailers and changed files, which closes like the diff panel.
* [0008](docs/decisions/0008-live-updates.md): A `notify` file watcher (skipping gitignored paths, falling back to polling git every 5 s or more) gathers changes into `files`, `index` and `refs` events pushed over Server-Sent Events, the browser fetches affected reviews again, and boxes whose color changes play a short CSS "fairy dust" animation.
* [0009](docs/decisions/0009-listening-beyond-localhost.md): A `--host <IP>` option lets vugent listen beyond loopback, where every request needs a random token generated at each start (never user-chosen), given once as `?token=` and then kept in an HttpOnly, port-named cookie, over plain HTTP with a warning that points to SSH port forwarding.
* [0010](docs/decisions/0010-diff-view.md): The diff panel switches between unified and side-by-side (S, remembered, side-by-side on wide windows by default), receives whole files with unchanged stretches folded in the browser (A unfolds all), and highlights the old and new files whole with tree-sitter, sent as text-and-class pieces at blob-ID URLs the browser caches as immutable.
* [0011](docs/decisions/0011-diagram-layout.md): The user arranges boxes in layers (Model, Business, Storage, Entry to start; renamable, reorderable, extendable) drawn as rings by default or as top-down or left-right bands (with today's folder view as a fourth choice), positions are saved per user in a gitignored `.vugent/layout.json` and never move on their own, folders that hold files stand in for packages until plugins exist, and vugent places only unplaced boxes, by folder names now and by known libraries and imports once plugins exist, or else in an Unsorted area.
