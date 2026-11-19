# Show a header for every review that opens into a detail page

## Context and Problem Statement

ADR 0005 lets the user step through the branch, the uncommitted work and each commit from a sidebar. When a commit
is selected, the canvas shows what changed but not why. The commit message, which is where an agent explains its
change, and the commit's metadata are not shown anywhere. Nor is it shown which branch the branch row is compared
against, or how much changed in total.

Where are the message and metadata shown, how does the user get to the full message, and what is shown for rows
that are not commits?

Issue #25 describes the feature.

## Considered Options

Where the summary goes:

* In the sidebar row, expanded under the selected commit
* A header across the top of the canvas
* Only in a detail page

How the full message is opened:

* A button or a click on the header
* Scrolling into the header, with an elastic pull, like the way a diff is closed (ADR 0003), plus Enter

How the backend serves a commit's details:

* Add them to every commit in `GET /api/commits`
* A separate `GET /api/commit?rev=<sha>`, cached by the browser

## Decision Outcome

Chosen options: **a header across the top of the canvas**, **scrolling into the header, plus Enter**, and **a
separate `GET /api/commit`**.

A header keeps the sidebar a compact list that J and K move through quickly, and keeps the summary next to the
changes it describes. Scrolling into it continues the semantic zoom of ADR 0003: zooming in on a file opens its diff,
and zooming in on the header opens the message. A separate request keeps `/api/commits`, which is fetched on every
focus, small; because a commit never changes, the browser keeps its answer.

### The header

A compact box across the top of the canvas, to the right of the sidebar, two lines high and never scrolled:

```
┌──────────────────────────────────────────────────────────────┐
│ feat: pick what to review from a commit history sidebar      │
│ 2aa7cf2 · Author Name · 3 hours ago · +412 −87               │
└──────────────────────────────────────────────────────────────┘
```

Every row gets one, so the layout does not jump on J and K:

| Row | First line | Second line |
|---|---|---|
| A commit | its subject | short SHA · author · relative author date · line counts |
| The branch | its name | "N commits since `main` @ abc1234" · line counts |
| Uncommitted, unstaged or staged changes | the row's label | "N files changed" · line counts |

The line counts are those of the review's tree: lines added and removed in every file.

### The detail page

* **Opening.** Scrolling up over the header, which is the wheel direction that zooms in on the canvas, pulls the
  header down like a rubber band; once it is pulled far enough, it snaps and the page opens. A single stray wheel
  tick does not open it. Enter opens it from the keyboard. J and K do nothing while it is open, as with a diff.
* **Closing.** As with the diff panel: scrolling on past the end, which pulls the elastic arc of ADR 0003, or Esc.
  Both pages share the same elastic code, so they behave the same.
* **A commit** shows its subject, the rest of the message as plain text in a readable width, the full SHA with a copy
  button, the author and author date, and the committer and commit date only when they differ from the author's.
  Trailers, such as `Co-Authored-By`, `Closes` and `Refs`, are shown as chips; issue references such as `#25` link to
  the issue when `origin` is on GitHub.
* **The branch** shows the commits of the branch; clicking one selects its row.
* **Every page** lists the changed files with their line counts. Clicking one opens its diff over the page; closing
  the diff returns to the page.

The diff panel's title bar shows the commit's subject on one line, so the reader keeps the context while reading a
diff.

### API

* `GET /api/commit?rev=<sha>` takes a commit's full SHA and returns its subject, its message without the trailers,
  the trailers, and its author and committer with their dates. It is read with `git show -s`. It is served with
  `Cache-Control: immutable`.
* `GET /api/commits` also returns `base`, the main branch and the short SHA of the merge-base (null when there is no
  branch row), and `github`, the repository's web address when `origin` is on GitHub (null otherwise).

### Consequences

* Good, because the reason for a change is one gesture away from the change itself.
* Good, because every row has a header of the same size, so the canvas does not move when the selection does.
* Good, because opening and closing both pages uses the same elastic gesture, so it is learned once.
* Bad, because the header covers a strip of the canvas. It is kept to two lines, and the canvas leaves room for it
  when fitting the tree.
* Bad, because opening by scrolling over the header is easy to miss; Enter and the hint on the header are offered as
  the obvious ways in.
