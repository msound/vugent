# The layout is a committed `.vugent.yaml` of layers, each an ordered list of boxes

## Context and Problem Statement

ADR 0011 made the canvas the user's own map. Boxes are arranged in layers, every box keeps a saved x and y, and the
arrangement lives in a gitignored `.vugent/layout.json`, so it is personal and never shared.

On a team, though, the arrangement of layers is an architecture decision. The architect or tech lead decides which
package belongs in which layer, and everyone else should review against the same map. When a teammate disagrees
with a placement, the change should go through a pull request like any other, so that it is discussed, approved
and kept in history. A personal file can't do that, and two teammates reviewing the same pull request would see
two different maps.

Once the layout is committed, saving pixel positions becomes a problem too. The smallest nudge of the mouse changes
a number, dirties the file and ends up in a commit. Such diffs say nothing a reviewer can judge.

Where is the layout kept, what does it record, and how do boxes find their place in it? This ADR replaces the
Storage and Positions parts of ADR 0011, and the parts of it that depend on them. Layers, roles, shapes, folders
standing in for packages, guessing layers, and scale are unchanged.

## Considered Options

Where the layout is kept:

* `.vugent/layout.json` in the reviewed folder, kept out of git (ADR 0011)
* A committed file at the root of the reviewed folder

What the layout records about a box:

* Its x and y, or its position along and across its layer (ADR 0011)
* Its layer and a number, such as a weight or sort order, that orders boxes within the layer
* Its layer, by being listed in that layer's ordered list of boxes

File format:

* JSON
* TOML
* YAML

## Decision Outcome

Chosen options: **a committed `.vugent.yaml` at the root of the reviewed folder**, **each layer as an ordered list
of boxes**, and **YAML**.

* Committing the layout makes it the team's map. Changing it is a pull request that the architect approves, and git
  keeps its history.
* Listing boxes in order records only what the user meant: which layer a box is in, and what it sits next to. A
  drag that doesn't change either writes nothing. Moving a box shows up in a diff as one line removed and one line
  added, which a reviewer can read as "`internal/cache` moved from Business to Storage". Weights would have to be
  renumbered or split into fractions to insert a box between two others, and a changed weight says nothing to a
  reviewer.
* A file at the root, rather than a `.vugent/` folder, is easy to find and is a single file.
* YAML reads best as a file of ordered lists: no brackets or trailing commas, and one box per line. YAML's
  well-known traps, such as a folder named `false`, `no` or `2024` being read as something other than text, are
  avoided by always quoting paths and layer names (see Writing below).

### The file

```yaml
version: 1
shape: rings
layers:
  - name: "Model"
    role: model
    boxes:
      - "internal/domain"
  - name: "Business"
    role: business
    boxes:
      - "internal/service"
  - name: "Storage"
    role: storage
    boxes:
      - "internal/store"
      - "internal/store/sql"
  - name: "Entry"
    role: entry
    boxes:
      - "cmd/server"
      - "internal/http"
```

| | Decision |
|---|---|
| `version` | The format version, starting at 1. |
| `shape` | `rings`, `top-down` or `left-right`. The shape is part of the team's map, so it is shared. |
| `layers` | In the user's order, the first being the innermost ring, the top band or the left band (ADR 0011). Each has a `name`, an optional `role` (`model`, `business`, `storage` or `entry`; added layers have none) and `boxes`. |
| `boxes` | Paths of top-level boxes, in order along the layer. Today a path is a folder that stands in for a package (ADR 0011), with `.` for the reviewed folder itself. Packages get their own ids once plugins exist (#8). |
| Writing | Vugent always writes the file the same way: the keys in the order above, one box per line, two spaces of indent, no comments. Paths and layer names are always in double quotes, so a folder or layer named `false`, `no`, `null` or `2024` stays text. `shape` and `role` are fixed words and are not quoted. Saving a layout that hasn't changed leaves the file as it was, so it never shows a diff for nothing. |
| Hand edits | Allowed. Comments and formatting are lost the next time vugent saves. When reading, a path or name that YAML parses as something other than text, such as an unquoted `false`, is an error that names the line, rather than being turned back into text, which might not give back what the user typed (`1.10` would come back as `1.1`). |
| No file | Vugent uses the four layers of ADR 0011 and the rings shape, and writes nothing until the user changes something, so opening a repository never creates a file in it. |
| Not readable | If the file can't be parsed, for example because it has merge conflict markers, the canvas shows the folder view with a warning naming the file and the error, and nothing can be dragged. Vugent never overwrites a file it couldn't read. |
| Duplicates | A box listed twice, for example after a merge, stays in the first place it is listed. The next save removes the other. |
| Missing boxes | A listed path that doesn't exist in the working tree is ignored while drawing and removed on the next save. Each branch carries its own file, so nothing is lost: switching branches brings back that branch's layout. |
| Not listed | A box that isn't in any layer goes to Unsorted, unless a guess places it (ADR 0011). A guess is shown but not written, so a better guess later doesn't change the file. It is written once the user drops the box somewhere. |

### Positions

* Nothing in the file is a pixel. Where a box appears is computed from its layer, its place in the list, and the
  sizes of the boxes before it. How boxes flow along a ring or band is left to the layers story (#44).
* Dragging a top-level box drops it into a slot: before, between or after the boxes of a layer, or into Unsorted.
  While dragging, a marker shows the slot. A drop that leaves the box's layer and order as they were writes
  nothing.
* Boxes inside a top-level box are laid out automatically, the same way every time for the same contents, and
  can't be dragged. None of their positions are saved. This may be revisited once there is feedback, or once classes
  exist.
* Unsorted is ordered by path.
* The folder view (ADR 0011) has no saved positions either. It is laid out automatically, as it is today.
* A box can move without anyone dragging it: when a box before it in the same layer grows or shrinks, or a box is
  added or removed, the boxes after it slide along. Their order never changes, and order is what people remember:
  "store sits next to cache, in Storage".

### Reading and saving

* The file is read from the working tree for every review, so that the map is the same for every commit, the
  branch and the working tree, and a change to the layout shows on the canvas right away.
* The server reads and writes the file with `GET` and `PUT /api/layout`. The browser sends and receives JSON, and
  the server converts it to and from YAML. The browser saves after every change, and if two browsers save, the last
  save wins.
* The file is an ordinary file in the repository, so the watcher reports changes to it as a `files` event
  (ADR 0008). The browser fetches the layout again on a `files` event whose paths include `.vugent.yaml`, or that
  carries no paths. This covers the user's own saves, hand edits and `git pull`.
* A change to the layout is a change in the review, and shows on the canvas like any other file.

### Personal settings

Which view is open (folders and files, or layers), the zoom, and which folders are collapsed are how one person
looks at the map, not part of it. They are kept in the browser's local storage.

### Left for later

* Showing a change to `.vugent.yaml` in the review as moved boxes ("3 boxes moved to Storage") rather than as a
  diff of the file.
* Moving a box to its new path when git detects a rename. Until then, renaming a package sends it to Unsorted, and
  the pull request that renames it should place it again.
* An arrange mode that must be switched on before boxes can be dragged, to guard against accidental drags. Without
  one, an accidental drag shows up in `git status` and can be undone with `git checkout .vugent.yaml`. The drag and
  drop story (#43) decides.

### Consequences

* Good, because the team shares one map, reviewed and approved like any other change, with its history in git.
* Good, because diffs of the layout are short and readable, and a drag that changes nothing writes nothing.
* Good, because the file lists only top-level boxes, so it stays small even for a service of 10,000 files.
* Good, because there are no pixel positions to keep stable, so there is no free-space search, no pushing of
  neighbours, and no need for Tidy or Reset. Reset is `git checkout .vugent.yaml`.
* Bad, because boxes can slide along their layer when a box before them changes size, so the map is stable in
  order but not in pixels.
* Bad, because vugent adds a file to the user's repository once they arrange anything, and every rearrangement is
  a change to commit.
* Bad, because a user can no longer keep a personal map that differs from the team's.
* Bad, because the boxes inside a package can't be arranged by hand.
