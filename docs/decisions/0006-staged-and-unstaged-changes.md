# Split uncommitted changes into staged and unstaged changes while something is staged

## Context and Problem Statement

ADR 0005 put a "Staged changes" row (`HEAD` → index) next to "Uncommitted changes" (`HEAD` → working tree). Staged
changes are uncommitted too, so "Uncommitted changes" includes everything in "Staged changes". The two labels make
them look like two separate sets, which is misleading. ADR 0005 also judged an unstaged-only review rarely useful,
but it fits a common way of working: review, `git add` what looks good, then look at what is left.

Which rows show uncommitted work, and what does each compare?

Issue #24 describes the problem.

## Considered Options

* Keep "Uncommitted changes" and "Staged changes", and only rename the first, such as "All uncommitted changes"
* Show "Uncommitted changes", "Staged changes" and "Unstaged changes" while something is staged
* Match `git status`: "Uncommitted changes" while nothing is staged, and "Unstaged changes" plus "Staged changes"
  while something is

## Decision Outcome

Chosen option: **match `git status`**, because its rows never overlap in what they claim to show, and they use the
words people already know from git. Three rows put two overlapping views next to each other again, and a rename does
not give the unstaged-only view.

| Situation | Rows |
|---|---|
| Nothing staged | **Uncommitted changes**: `HEAD` → working tree, untracked files included (as in ADR 0005) |
| Something staged | **Unstaged changes**: index → working tree, untracked files included, then **Staged changes**: `HEAD` → index |

When nothing is staged, the two views would be the same, so that case does not change. The list stays newest first:
unstaged changes are the latest work, and staged changes are one step closer to a commit, so they sit between the
unstaged changes and the commits. "Unstaged changes" is only listed when there are some, like every other row. The
branch row (merge-base → working tree) and the commit rows of ADR 0005 are unchanged, and the branch row still shows
everything at once.

* **A file staged and then edited again** appears in both rows, each showing its own part of the change.
* **Opening.** The page opens on the first row under the branch row (the first row when there is none), so on the
  newest uncommitted work, or on the newest commit when nothing is uncommitted.
* **Refreshing.** When the selected row of uncommitted work is gone, for example "Uncommitted changes" after a
  `git add`, the first row under the branch is selected again, rather than the newest commit.

### How unstaged changes are read

| | Unstaged changes |
|---|---|
| Every file | `git ls-files` plus untracked files |
| Changes | `git diff` without a base |
| Line counts (ADR 0004) | read from disk |
| An unchanged file's content | read from disk |

These are the working tree reads of ADR 0005 with the index as the base. Without commits, the index is still the
base.

### API

* `GET /api/commits` also returns `unstaged`: whether something in the folder differs from the index, or is
  untracked.
* `GET /api/tree` and `GET /api/diff` also take `rev=unstaged`. `rev=worktree` keeps working while something is
  staged, although the sidebar does not offer it then.

### Consequences

* Good, because no two rows of uncommitted work overlap, so the labels say what they show.
* Good, because what is left to review after a `git add` has its own view.
* Bad, because on the main branch, where there is no branch row, no single row shows everything uncommitted while
  something is staged. This trade-off is accepted.
