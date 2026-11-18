# Review uncommitted changes against HEAD

## Context and Problem Statement

Vugent shows a block diagram of what changed so that the user can review it. Before anything can be drawn, vugent
must know what is being compared against what. Over time, users will want several review modes (#2): uncommitted
work, all commits on a branch, or a single commit picked from history.

v0.1.0 must ship quickly. It handles a single service: the user runs `vugent .` from the service's root
directory, which may be the root of the repository or a subdirectory of it. The main use case is keeping up with
what an agent is doing right now, and an agent's work in progress is mostly uncommitted.

Which comparison should v0.1.0 offer, and how should it be modeled so that further review modes can be added later
without a redesign?

## Considered Options

* Unstaged changes only (`git diff`, comparing the working tree against the index)
* Staged changes only (`git diff --cached`, comparing the index against HEAD)
* All uncommitted changes: the working tree, including untracked files, compared against HEAD
* Branch changes: the working tree compared against the merge-base with the main branch

## Decision Outcome

Chosen option: **all uncommitted changes compared against HEAD**, because it shows everything an agent has done
since the last commit, whether or not the agent staged it, and it needs no main-branch detection.

A review is modeled as a pair, **(base, target)**:

* *base* is a commit, or the empty tree.
* *target* is a commit, or the working tree.

v0.1.0 has a single mode, where base is `HEAD` and target is the working tree. Later modes become other pairs:
branch review is (merge-base, working tree or `HEAD`), and single-commit review is (parent, commit). Those modes,
including main-branch detection and the handling of merge commits, are left to a later ADR.

Details for v0.1.0:

| Area | Decision |
|---|---|
| Tracked changes | `git diff HEAD -M --name-status -z` lists changed files; `--numstat` gives line counts. Staged and unstaged changes are combined and not told apart. |
| Untracked files | `git ls-files --others --exclude-standard -z`. Each untracked file is shown as added. Its diff is produced with `git diff --no-index /dev/null <file>`. Ignored files are left out. |
| Scope | Every git command is restricted to the directory vugent was started in, using `--relative` or a `-- .` pathspec, so running vugent in a monorepo subdirectory shows only that service. |
| Full tree | The diagram also shows unchanged folders and files, listed with `git ls-files -z` plus the untracked files. Deleted files appear even though they are no longer on disk. |
| No commits yet | When `HEAD` does not exist yet (an unborn branch), the base is the empty tree, so every file is shown as added. |
| Detached HEAD | Supported without any special handling, because only `HEAD` is used. |
| Binary files | Listed as changed with no line counts. The diff view says the file is binary instead of showing its contents. |
| Submodules | Shown as a single entry. Vugent does not look inside a submodule. |
| Refresh | The comparison is computed again on every request. Vugent never writes to the index or the working tree. |

### Consequences

* Good, because the user sees everything the agent has changed, whatever the agent did or did not stage.
* Good, because no main-branch detection or extra configuration is needed. `vugent .` just works.
* Good, because the (base, target) model lets branch and single-commit reviews be added later without reworking
  the diagram or the diff view.
* Good, because vugent only reads from the repository, so it is safe to run while an agent is working.
* Bad, because work that the agent has already committed disappears from view. Reviewing an agent that commits
  often has to wait for the branch review mode.
* Bad, because a move that has not been staged appears as a deletion plus an untracked file, not as a rename.
  Git only detects renames between tracked files, and vugent will not run `git add -N` because it never writes to
  the index.
* Bad, because the difference between staged and unstaged changes is hidden, which matters to users who stage
  changes as they review them.
