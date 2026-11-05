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

When creating issues, use either of the labels: enhancement, bug.

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



## Architecture decisions

Decisions are recorded in docs/decisions/ using the MADR template (docs/decisions/0000-template.md).
Below is a one sentence summary of each. Read the full ADR when working in an area it covers, or before
proposing something that contradicts it. When a new decision is made, add an ADR with the next number and
a summary line here.

* [0000](docs/decisions/0000-template.md): A minimal MADR (Markdown Architecture Decision Record) template to use.
* [0001](docs/decisions/0001-tech-stack.md): Rust core (axum, cargo-dist) with tree-sitter analysis via WASM component plugins, a Vite + Svelte 5 frontend with Cytoscape.js/elkjs, server-side highlighting, and the `git` CLI as the only external requirement.
