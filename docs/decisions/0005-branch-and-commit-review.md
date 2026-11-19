# Review the branch, staged changes and single commits from a history sidebar

## Context and Problem Statement

ADR 0002 gave v0.1.0 a single review: every uncommitted change compared against `HEAD`. Work that an agent has
already committed disappears from view, and the difference between staged and unstaged changes is hidden. ADR 0002
modeled a review as a (base, target) pair so that more reviews could be added later, and left branch review and
single-commit review to a later ADR.

Which reviews are offered next to the uncommitted changes, and what does each one compare? How does the user pick
one? How is the main branch found, and what happens on the main branch itself, at a merge commit, or at the
repository's first commit?

Issue #10 asked these questions and issue #23 describes the feature.

## Considered Options

Picking a review:

* CLI flags, such as `vugent --commit abc1234`, one review per run
* A list in the UI, switched while vugent runs

Staged changes:

* A "Staged changes" review (index against `HEAD`) next to "Uncommitted changes" (working tree against `HEAD`), which
  includes it
* "Staged" and "Unstaged" reviews that do not overlap

Branch review:

* The merge-base against `HEAD`, committed work only
* The merge-base against the working tree, uncommitted work included

What a commit shows on the canvas:

* The file tree of today's working tree, colored by the commit's changes
* The file tree as it was at that commit, colored by the commit's changes

## Decision Outcome

Chosen options: **a list in the UI**, ~~**"Staged changes" next to "Uncommitted changes"**~~ (superseded by
[ADR 0006](0006-staged-and-unstaged-changes.md): "Staged" and "Unstaged" changes while something is staged), **the
merge-base against the working tree**, and **the file tree as it was at that commit**.

A list in the UI is chosen because stepping through an agent's commits one by one, without restarting vugent, is
the point of the feature. Uncommitted plus staged is chosen because "everything the agent did" stays the default
view and "what is about to be committed" is the extra one; an unstaged-only review is rarely what a reviewer asks
for. The branch review includes uncommitted work so that it really is the sum of every row under it, which lets the
list show it as their parent. A commit shows the tree as it was at that commit, because files that a later commit
added would otherwise appear in a commit that never had them.

### The sidebar

A panel floats on the left of the canvas:

```
feature-x · 5 commits
    Uncommitted changes
    Staged changes
    abc1234 feat: add sidebar
    def5678 fix: handle root commit
```

| Row | Base | Target | Shown when |
|---|---|---|---|
| Branch (its name, or the short SHA on a detached `HEAD`, and its number of commits) | merge-base with the main branch | working tree | the branch has at least one commit in the reviewed folder |
| Uncommitted changes | `HEAD` | working tree, untracked files included | something in the reviewed folder is uncommitted |
| Staged changes | `HEAD` | index | something in the reviewed folder is staged |
| A commit: short SHA and subject | its first parent | the commit | always |

* **Indentation.** When the branch row is shown, every other row is indented under it. There is one level only:
  "Staged changes" is not nested under "Uncommitted changes", although it is part of it.
* **Which commits.** `git log --first-parent <merge-base>..HEAD -- .`: newest first, only commits that touch the
  reviewed folder, and never the commits of a branch that was merged in.
* **No branch.** On the main branch itself, or when no main branch is found, the merge-base is `HEAD` or missing.
  The branch row is left out and the list shows the last 50 commits that touch the reviewed folder, without
  indentation.
* **Nothing at all.** A repository without commits or changes shows "Nothing to review" in the panel.
* **Keys.** J selects the next row down (older), K the next row up. They do nothing while a diff is open, so they
  never fight with reading it, or when a modifier key is held. A click selects a row too. The canvas loads shortly
  after the selection stops moving, so holding J does not load every commit on the way.
* **Opening.** The page opens on "Uncommitted changes", or on the newest commit when there are no uncommitted
  changes. The selection is kept in the URL (`?rev=…`), so a reload keeps it.
* **Refreshing.** The list is fetched again when the window regains focus. If the selected row is gone, because
  the agent committed or amended, the newest commit is selected.
* **Switching** to another row closes an open diff and fits the new tree on the canvas.

### Finding the main branch

The first of these that exists is the main branch: the `--base <ref>` CLI flag, `origin/HEAD`, `main`, `master`.
The merge-base is `git merge-base HEAD <main>`. On a local `main` that is ahead of `origin/main`, the branch row
shows the commits that have not been pushed yet.

### Edge cases

* **Merge commits** are compared against their first parent, so a merge shows what it brought into the branch.
* **The first commit** of the repository is compared against the empty tree, so every file is added.
* **No commits yet:** "Uncommitted changes" and "Staged changes" use the empty tree as base, as in ADR 0002.
* **Commits are identified by their full SHA** in the URL and the API, never by `HEAD~n`, which points at a
  different commit as soon as the agent commits again.

### How each target is read

| | Working tree | Index | Commit |
|---|---|---|---|
| Every file | `git ls-files` plus untracked files | `git ls-files` | `git ls-tree -r <commit>` |
| Changes | `git diff <base>` | `git diff --cached <base>` | `git diff <base> <commit>` |
| Line counts (ADR 0004) | read from disk | `git diff --numstat` of the empty tree against the index | `git diff --numstat` of the empty tree against the commit |
| An unchanged file's content | read from disk | `git cat-file blob :./<path>` | `git cat-file blob <commit>:./<path>` |

Untracked files belong to the working tree only. Everything else in ADR 0002 (scope to the reviewed folder, renames,
binary files, submodules, never writing to the index) applies to every target.

### API

* `GET /api/commits` returns the branch (or none), whether there are uncommitted and staged changes, and the list of
  commits.
* `GET /api/tree` and `GET /api/diff` take `rev`: `worktree` (the default, as in v0.1.0), `index`, `branch`, or a
  commit's full SHA. Anything else is rejected before it reaches git.

### Consequences

* Good, because committed work stays reviewable, one commit at a time or as a whole branch.
* Good, because staged changes can be reviewed on their own, which ADR 0002 listed as a gap.
* Good, because every row is another (base, target) pair, so the canvas, the colors and the diff panel are reused
  unchanged.
* Bad, because a commit's line counts come from reading every file of the reviewed folder at that commit. One
  service is small enough for this, and the counts of a commit never change, so they can be cached later.
* Bad, because the list is only refreshed on focus. An agent's new commit shows up when the user returns to the
  window, not while they watch; live updates are left to issue #6.
* Bad, because only first-parent history is listed. Commits that arrived through a merge are only seen as part of
  the merge.
