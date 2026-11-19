# Push live updates from a file watcher over Server-Sent Events

## Context and Problem Statement

Vugent is meant to be watched while a coding agent works. Today the server keeps no state: every request runs git
again, and the browser only fetches the sidebar again when its window regains focus (ADR 0005). An agent's edits and
commits show up when the user returns to the window, not while they watch.

As the agent touches a file, its box should take on its color in the browser, and new commits, staged changes and
branch switches should appear in the sidebar on their own. How are changes detected, how are updates pushed to the
browser, and how is the activity shown?

Issue #6 describes the problem. Layout stability while boxes appear and disappear belongs to the diagram layout ADR
(#5); vugent's own storage belongs to the caching ADR (#7).

## Considered Options

Detecting changes:

* Poll git (`git status`, `HEAD` and the refs) every few seconds
* Watch the file system with the `notify` crate, and poll only when watching is not possible
* Use git's file system monitor (`core.fsmonitor`)

Pushing updates:

* The browser polls the API
* Server-Sent Events
* WebSockets

What an update carries:

* The whole new tree of every affected review
* Only what changed (a patch of the tree)
* Which reviews and paths changed, and the browser fetches them again

## Decision Outcome

Chosen options: **watch with `notify`**, **Server-Sent Events**, and **say what changed, then fetch again**.

* Watching reacts within a fraction of a second at no cost while nothing changes. Polling git at the same pace runs
  `git status` several times a second in large repositories. Git's file system monitor only exists on macOS and
  Windows, has to be turned on in the user's repository, and only speeds up `git status`; it does not tell vugent
  anything.
* Server-Sent Events were already chosen in ADR 0001. Updates only flow from the server to the browser, so the
  second direction of a WebSocket is not needed, and `EventSource` reconnects on its own.
* Saying what changed reuses `/api/commits`, `/api/tree` and `/api/diff` unchanged, and keeps the server free of
  per-browser state. Pushing whole trees sends every affected review, including the ones nobody is looking at, and
  patches would need the server to remember what each browser has.

### Detecting changes

| | Decision |
|---|---|
| Platforms | Linux (inotify), macOS (FSEvents) and Windows (ReadDirectoryChangesW), through the `notify` crate. |
| What is watched | The reviewed folder, and in the git directory only `HEAD`, `index`, `refs/` and `packed-refs`. In a linked worktree `HEAD` and `index` are in the worktree's own git directory and the refs in the common one; both are found with `git rev-parse --git-dir --git-common-dir`. |
| Ignored | Everything else under `.git`, every `*.lock` file (git writes `index.lock` and moves it into place, which is the event that counts), and gitignored paths, matched with the `ignore` crate. Changes to a `.gitignore` reload the matcher. |
| Gitignored folders | On Linux every folder needs its own watch, so gitignored folders such as `node_modules/` and `target/` are not watched at all, and watches are added for new folders as they appear. macOS and Windows watch the whole tree at no extra cost, and their events in gitignored paths are dropped. |
| Rapid writes | Events are gathered until the folder has been quiet for about 150 ms, but for no longer than about 1 s, so an agent that writes without pause still shows progress. One batch is handled at a time; events that arrive meanwhile form the next batch. |

The watcher only starts while at least one browser is connected to the event stream, and stops when the last one
leaves.

### What changes when

| Event | What it means | Reviews affected |
|---|---|---|
| `files` | Files in the reviewed folder changed. Carries their paths, up to 1,000; a larger batch only says that too many changed. | Uncommitted changes, unstaged changes, the branch, and the sidebar (whether anything is uncommitted or unstaged). |
| `index` | The index changed, such as after `git add`. | Staged changes, unstaged changes, uncommitted changes, and the sidebar. |
| `refs` | `HEAD` or a ref changed: a new commit, a branch switch, a rebase, a reset. | The sidebar and every review except single commits. |

A commit's review never changes, so it is never fetched again. A branch switch also changes many files, and its
`files` and `refs` events usually arrive in the same batch.

There is no incremental analysis yet: the browser fetches the selected review again, and the server runs git again,
as it does today. For one service this is fast enough. Incremental re-analysis of imports, once there are language
plugins, is left to the plugin architecture (#8) and caching (#7) ADRs.

### Pushing to the browser

* `GET /api/events` is a Server-Sent Events stream of the events above, as JSON, such as
  `event: files` with `{"paths": ["src/lib.rs"]}`.
* When the stream opens, including after a reconnect, the browser fetches the sidebar and the selected review again,
  since it may have missed events. This replaces the refresh when the window regains focus.
* On an event, the browser fetches the sidebar again if it is affected, and the selected review if it is affected.
  An open diff panel fetches its diff again if its file is among the paths, or if the event carries no paths. The
  panel keeps its scroll position.
* While boxes appear and disappear, the canvas is laid out from scratch as it is today. Keeping it stable is left to
  #5.

### When the operating system says no

* When the watcher cannot start, usually because Linux ran out of inotify watches, vugent falls back to polling. The
  limit is shared by every program the user runs, so this can happen partway through adding watches, for example in
  a large repository such as Kubernetes (about 6,000 tracked folders) on a kernel older than 5.11, whose default
  limit is 8,192. The watches already added are then removed, and the whole folder is polled: watching part of it
  would silently miss changes in the rest.
* A poll compares the output of `git status --porcelain=v2 -z --untracked-files=all` and the values of `HEAD` and
  the refs with the last ones, and sends a `files`, `index` or `refs` event without paths when they differ. Polls
  are 5 s apart, or 10 times as long as the last poll took if that is longer, so polling a large repository never
  keeps a CPU core more than about a tenth busy. Live updates are for keeping an eye on an agent, so a few seconds
  of delay are fine.
* Vugent then prints a warning to stderr, once, saying that it ran out of file watches and is polling instead, and
  how to avoid it: raise the limit with `sudo sysctl fs.inotify.max_user_watches=524288`, made permanent with a file
  in `/etc/sysctl.d/`, or review a subfolder, which only watches that folder. The UI shows a small note that changes
  are being polled.
* A `--poll` flag starts in polling mode, for file systems that deliver no events, such as network drives or `/mnt/c`
  under WSL.
* Vugent's own files (#7) must be stored outside the watched paths, so that writing them never sends an event.

### Showing activity

A box whose line counts changed gets a short animation, after which it sits in its new color as usual. The browser
finds these boxes by comparing the tree it had with the one it fetched.

* **Fairy dust when the color changes.** When a box's hue or shade changes (ADR 0004), such as white to green, green
  to orange, or a lighter shade to a darker one, it bumps slightly in scale, its new color fades in, and a handful
  of small particles in that color scatter outward and fade. New files appear with the same effect, and red boxes
  get it in red.
* **A soft glow when only the counts change.** An agent saving the same file over and over would otherwise sparkle
  without end, so more lines in the same color only give a short glow in that color.
* **Closed folders.** A changed file inside a folder too small on screen to be open (ADR 0003) animates that folder
  instead, so activity is never hidden.
* **Only boxes on screen** animate, so a branch switch that changes hundreds of files does not set off hundreds of
  effects.
* **Reduced motion.** With `prefers-reduced-motion`, there is no scaling or particles, only the color fading in.
* The animation is CSS only (`@keyframes` on `transform`, `opacity` and `box-shadow`), with no library. Particle
  count, timing and size are tuned when it is built. If fairy dust does not look good, a plain fade-in or a glow ring
  can be tried in a later enhancement.

### Consequences

* Good, because the user sees the agent's work arrive as it happens, without switching windows.
* Good, because the API and the server's git reads are reused unchanged; the only addition is the event stream.
* Good, because gitignored build output and dependencies cost neither inotify watches nor refreshes.
* Bad, because every update runs git again for the whole reviewed folder. This is accepted until it is measured to be
  too slow.
* Bad, because the layout may jump while boxes appear and disappear, until #5 decides how to keep it stable.
* Bad, because in polling mode updates lag by 5 s or more and do not say which files changed, so an open diff panel
  fetches its diff on every update.
