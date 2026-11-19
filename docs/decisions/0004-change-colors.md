# Change colors from the share of lines changed

## Context and Problem Statement

The canvas from ADR 0003 colors each folder and file box by how it changed in the review: green for added, orange
for modified, red for deleted and white for unchanged. A folder is orange if anything inside it changed. This says
nothing about how much changed, so a one-line fix and a full rewrite look the same, and a folder holding only new
files is orange instead of green.

The color of a box should show both what kind of change happened and how much of the box it covers, at every level
of the folder tree. git reports each file's change as lines added and lines removed. It has no notion of a changed
line, because an edit is a removed line followed by an added one.

How is a file's change measured and turned into a color? How does a folder get its color from its contents? How are
renames, binary files and submodules treated, and how do the colors work for color-blind users and in both themes?

Issue #4 asked these questions and issue #20 proposed answers.

## Considered Options

Measuring a change:

* Status only (added, modified, deleted), as ADR 0003 does
* Absolute line counts, for example "heavy editing" above 100 changed lines
* Lines changed as a share of the file's size

Shades:

* A continuous shade from the percentage
* Two shades, light and dark
* Three shades: light, medium and dark

A folder's color:

* The color of its most changed child
* The share of its files that changed
* The same rule as a file, applied to the summed line counts of every file inside it

Renames:

* A rename is always dark orange
* A rename is colored by its content changes alone, like a modified file

## Decision Outcome

Chosen options: **lines changed as a share of the file's size**, **three shades**, **the file rule applied to summed
line counts**, and **renames colored by their content changes**.

A share of the file's size is chosen because ten changed lines are a lot in a 20-line file and very little in a
2,000-line one, and because a completely new or deleted file comes out as exactly 100%. Three shades are chosen
because they are easy to tell apart at a glance, which a continuous shade is not, while still separating "a few
lines" from "most of the file" from "all of it". Summing line counts is chosen because one rule then covers files
and folders at every depth, and a folder of only new files comes out dark green with no special case.

### Hue

Each file and folder has four numbers: lines added, lines removed, its old line count (in `HEAD`) and its new line
count (in the working tree).

| Lines added | Lines removed | Hue |
|---|---|---|
| 0 | 0 | none (unchanged) |
| more than 0 | 0 | green |
| 0 | more than 0 | red |
| more than 0 | more than 0 | orange |

### Share

| Hue | Share |
|---|---|
| green | lines added ÷ new line count |
| red | lines removed ÷ old line count |
| orange | the larger of lines added and lines removed ÷ the larger of the old and new line counts |

A new file is 100% green and a deleted file is 100% red. For orange, a file whose every line was replaced is 100%,
and ten lines edited in a 100-line file (+10 −10) is 10%.

### Shade

| Share | Shade |
|---|---|
| less than 50% | light |
| 50% or more, but less than 100% | medium |
| 100% | dark |

### Folders

A folder sums the four numbers over every file inside it, at any depth, unchanged files included, and then uses the
same hue, share and shade rules as a file. So:

* A folder of only new files is dark green.
* A folder with one file added and another deleted is orange.
* A folder where one small file changed among many unchanged ones is light.

### Special cases

* **Line counts.** Only the new line count is read, by counting lines in the working tree file the way git does
  (a last line without a newline still counts). The old count follows from the diff: old = new − added + removed.
  A deleted file has a new count of 0.
* **Binary files and submodules** have no line counts, so each is counted as one line: added is dark green, deleted
  is dark red, changed is dark orange and unchanged is white. This matches how git shows a submodule change, as one
  removed and one added commit line.
* **Renames** are colored by their content changes, like a modified file. A rename with no content changes is
  unchanged, but its box still shows the old path. A rename is placed at its new path, so it counts toward the new
  folder only.

### Accessibility and themes

* Color is never the only cue. A file box shows its `+N −M` counts, a deleted file's name is struck through, and a
  folder box shows its summed `+N −M` next to its name.
* The three shades differ in lightness, not only in saturation, so they stay apart for color-blind users and in
  grayscale. Green and red are the hardest pair for the most common color blindness, and the counts above cover it.
* The fill carries the shade and the border carries the hue at full strength, so the hue is clear even on a light
  fill.
* Both themes define nine fill colors (three hues × three shades) as CSS custom properties. In the dark theme, dark
  means the strongest fill, not literally the darkest. Box text keeps a contrast of at least 4.5:1 on every fill.

### Where it is computed

The server adds each file's new line count to the file entries and computes the hue and shade of every file and
folder when it builds the tree, so the rules live in one place and are covered by Rust tests. The frontend only maps
hue and shade to CSS classes.

This replaces the "Colors" row of ADR 0003.

### Consequences

* Good, because the size of a change can be seen from far away, before any folder is opened.
* Good, because one rule covers files and folders at every depth, and new folders show up as dark green.
* Good, because the rules are pure arithmetic on four numbers and are easy to test.
* Bad, because every file in the reviewed folder is read to count its lines, not only the changed ones. One
  service's files are small enough for this, and the counts can be cached later if it is not.
* Bad, because a file with mostly additions and one removed line is orange, not green. The rule is kept simple on
  purpose, and the `+N −M` counts show the real balance.
* Bad, because a pure rename looks unchanged on the canvas, apart from its old path. A reviewer who wants to see
  moves at a glance would need a separate marker.
