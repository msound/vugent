# The canvas is the user's own map, arranged in layers and saved per user

## Context and Problem Statement

Tools such as GitHub list a pull request's changes in alphabetical order of folders and files. A reviewer reads
the HTTP handlers, the business logic, the model and the storage code in whatever order their paths happen to
sort. Vugent's aim is to show changes the way the developer thinks about the software: as layers. Then a reviewer
can read a change top-down (HTTP, business, model, storage), bottom-up, or inside-out like a hexagonal
architecture (core, business, storage, HTTP). A small, focused commit should light up a single area of the
canvas.

v0.1.0 shows folders and files to get a usable release out. In the end the canvas is meant to show packages and
classes, once language plugins analyze imports (#8). Today, `web/src/lib/layout.ts` packs the whole folder tree
from scratch with ELK's rectangle packing on every update (ADR 0003). One new file makes its folder bigger, which
pushes boxes around all the way up the tree. Every review is laid out on its own, so moving between commits
shuffles the canvas too. ADR 0008 left keeping the layout stable to this decision.

Who decides where a box goes? How does the layout stay put while boxes appear and disappear, and from one review
to the next? How is it stored, how does it scale to large services, and are boxes grouped? Issue #5 describes the
problem. ADR 0003 settles how boxes nest and open as the user zooms, and this ADR does not change that.

## Considered Options

Who decides where a box goes:

* Vugent, automatically, for example with ELK's layered algorithm along the direction of imports
* Vugent, automatically, but keeping boxes it placed before where they are
* The user, who arranges the canvas by hand; vugent places only the boxes the user has not placed yet

What boxes are grouped by:

* The folder tree, as today
* Layers, which the user puts boxes into, whatever folder they are in

How layers are drawn:

* Always as horizontal bands
* Always as concentric rings
* The user's choice of rings, top-down bands or left-right bands, next to the folder tree as it is today

Where the arrangement is kept:

* A file committed to the repository and shared with the team
* Vugent's own storage, outside the repository
* `.vugent/layout.json` in the reviewed folder, kept out of git

Boxes vugent cannot place:

* A layer chosen by default, such as the middle one
* An "Unsorted" area beside the layers

## Decision Outcome

Chosen options: **the user decides, and vugent places only what the user has not**; **layers**; **the user's
choice of ~~the folder tree,~~ rings or bands, rings by default** (the folder view is removed by
[ADR 0012](0012-committed-layer-layout.md)); ~~**`.vugent/layout.json`, kept out of git**~~ (superseded by
[ADR 0012](0012-committed-layer-layout.md): a committed `.vugent.yaml`); and **an Unsorted area**.

* The canvas should match the developer's mental model of their software, and only the developer has that model.
  Like furniture in their own home, they put boxes where they want them: one user wants `package.json` out of the
  way, another wants it front and center. Any layout vugent computes on its own, however stable, will sometimes
  be wrong for them. ELK's layered algorithm can't draw rings, and it moves boxes whenever the graph changes.
* Layers are how people describe architecture (HTTP, business, model, storage), and they don't follow the folder
  tree. Layers also let a reviewer read a change in the order they choose.
* Top-down, bottom-up and inside-out reviewing are all better than alphabetical order, and none of them is right
  for everyone. Vugent only gives the user spaces to park boxes in. It doesn't decide which box goes where.
* ~~The layout is personal. Committing it would make it a team decision, and every save would show up as an
  uncommitted change in the review itself.~~ Superseded by [ADR 0012](0012-committed-layer-layout.md): the layout is the team's decision, so it is
  committed and changed through pull requests.
* Packages vugent can't place should be easy to see, so that the user places them. In a layer chosen by default,
  they would look as if vugent had placed them on purpose.

### Layers and shapes

| | Decision |
|---|---|
| Layers | Four to start, in this order: **Model, Business, Storage, Entry**. Each has a role of the same name. The user can rename any layer, change their order, and add more. A layer keeps its role when renamed or moved, and added layers have no role. Removing a layer moves its boxes to Unsorted. |
| Views | ~~Four icons switch between them: **folders and files**, today's canvas with no layers, where top-level boxes sit anywhere on the canvas; and three shapes of layers.~~ Superseded by [ADR 0012](0012-committed-layer-layout.md): there is no folder view, and the shape, saved in `.vugent.yaml`, is the only choice. |
| Shapes | **Rings** (the default; the first layer is the innermost ring), **top-down bands** (the first layer on top) and **left-right bands** (the first layer on the left). |
| Switching shape | Keeps the user's order of layers and draws the same boxes in the new shape. It doesn't reorder layers to suit the shape. |
| Switching view | ~~Boxes inside other boxes keep the same positions in every view, since what a box contains doesn't change. Only the top-level boxes are placed per view: at x and y in the folder view, in a layer in the others. The view is saved.~~ Superseded by [ADR 0012](0012-committed-layer-layout.md): boxes inside other boxes are laid out automatically, and there is no folder view to switch to. |
| Unsorted | An area beside the layers that holds every box that hasn't been placed in a layer. |
| Labels | Each ring or band shows its layer's name. |
| Size | A layer grows to hold the boxes in it. A ring's thickness and a band's depth fit its tallest stack of boxes. |

### Folders stand in for packages

Until a language plugin analyzes a service, the layer views have no packages. Top-level folders don't work in their
place: in a typical Go service the top-level boxes would be `cmd/` and `internal/`, and the layers worth placing,
such as `internal/handler` and `internal/store`, sit one level down, where the box-inside-its-parent rule below
would keep them.

So in the layer views a folder stands in for a package, as it is in Go, Java and Python: **every folder that holds
files directly is one top-level box**, labeled with its path (`internal/store`), with those files inside. The
reviewed folder itself is such a box when it holds files, such as `package.json` or `README.md`. A folder that holds
only folders, like `internal/` or `cmd/`, isn't drawn in the layer views. Its subfolders are boxes of their own, so
`internal/store` and `internal/store/sql` are two boxes, side by side.

~~The folder view keeps today's nested tree. Inside a folder, files keep the same positions in both views; in the
layer views, the space a subfolder takes in the folder view is left as a gap.~~ Superseded by
[ADR 0012](0012-committed-layer-layout.md): there is no folder view, and a box's files are laid out automatically.

When a plugin arrives, each folder box becomes a real package box with the same place in its layer. This fits Rust badly,
where a module is usually a file, such as `src/http.rs`, rather than a folder, and vugent's own code would be a
single box. That is accepted for a stand-in, and the Rust plugin replaces it.

### Positions

Superseded by [ADR 0012](0012-committed-layer-layout.md), except the first two bullets: a box is listed in its
layer's ordered list of boxes, with no pixel positions, and boxes inside another box are laid out automatically and
can't be dragged for now.

* Only top-level boxes move between layers and Unsorted. Boxes inside them go with them.
* A box never leaves what contains it. Like a method that can't be pulled out of its class, the canvas always shows
  the real structure. Folders and files are a stand-in until packages and classes exist, and follow the same rule.
* ~~Boxes are keyed by their id: `d:` or `f:` followed by the path today, with packages and classes added later. Each
  saved box records where it is:~~
  * ~~**A top-level box** records x and y for the folder view, and for the layer views its layer, or Unsorted, as a
    position along the layer, from 0 to 1, and a distance across it from the layer's inner edge. In a band these
    are the distance along the band and the depth within it. In a ring they are the angle and the distance out from
    the ring's inner edge. The same two numbers work for every shape, so switching shape redraws the same
    arrangement. Boxes that overlap in the new shape are pushed apart as little as possible, and the result is
    saved.~~
  * ~~**A box inside another** records x and y from its parent's top-left corner.~~
* ~~A box with a saved position **never moves on its own**.~~
* ~~A box without one is placed in free space inside its parent folder, or in Unsorted if it is at the top level. No
  other box moves. Its position is then saved like any other.~~
* ~~A removed box leaves a gap. A folder that outgrows its space grows right and down, and pushes only the boxes it
  would overlap, as little as possible.~~
* ~~The arrangement is the same for every review. A box keeps its place in every commit, the branch and the working
  tree. A box that doesn't exist in the selected review isn't drawn, and its saved position is kept, so switching
  branches and back restores it.~~
* ~~Nothing is packed again unless the user asks. **Tidy** packs one folder or layer from scratch, and **Reset**
  throws the whole saved arrangement away.~~

### Placing boxes automatically

Vugent places a box only when it ~~has no saved position~~ isn't listed in a layer ([ADR 0012](0012-committed-layer-layout.md)).

| Signal | Decision |
|---|---|
| Folder names | Until a language plugin covers a service, a folder box's name places it. The last part of its path, in any language: `handler`, `handlers`, `http`, `api`, `routes`, `controller` or `web` go into **Entry**; `service`, `services`, `usecase` or `app` into **Business**; `model`, `models`, `domain` or `entity` into **Model**; `store`, `storage`, `db`, `repository`, `repo`, `dao` or `persistence` into **Storage**. The list is kept in one place and grows as it is used. |
| Inside a folder | ~~ELK's rectangle packing lays out a folder the first time it is seen, and again on Tidy. After that, new boxes go into the first free space that fits them, found by vugent's own code, since ELK can't add boxes around ones that stay fixed.~~ Superseded by [ADR 0012](0012-committed-layer-layout.md): a folder's contents are laid out automatically, the same way every time for the same contents. |
| Known libraries | Each language plugin (#8) lists well-known libraries by role. A package that uses a database driver (MongoDB, PostgreSQL, MySQL, MariaDB and so on) goes into the **Storage** layer. A package that serves HTTP endpoints (in Go, `net/http`, gin, gorilla/mux and so on) goes into the **Entry** layer. |
| The import graph | Places the packages in between. A package that imports nothing in the project but is imported by others goes into **Model**. One that sits between Entry and Storage packages goes into **Business**. |
| Neither | The package goes into **Unsorted**. |

The guess uses roles, not layer names or order, so it keeps working after the user renames or reorders layers.
Folder names are a cheap guess that works in any language, so the rings aren't empty on a first visit. Library
fingerprints and imports replace it for each language that gets a plugin.

### Storage

Superseded by [ADR 0012](0012-committed-layer-layout.md): the layout is a committed `.vugent.yaml` at the root of
the reviewed folder, read from the working tree for every review, and changes to it are `files` events like any
other file.

* ~~`.vugent/layout.json` lives in the reviewed folder. It holds a format version, the shape, the layers with their
  names and roles, and every saved position. Next to it is a `.vugent/.gitignore` containing `*`, so the folder is
  never committed: the arrangement is per user and isn't shared with the team.~~
* ~~The server reads and writes the file with `GET` and `PUT /api/layout`, and the browser saves after every change.
  If two browsers save, the last save wins.~~
* ~~The watcher skips gitignored paths (ADR 0008), so saving the layout never sends a `files` event and never shows
  up in a review.~~
* ~~Positions of boxes that no longer exist are kept, which is cheap, until Reset.~~
* ~~The caching ADR (#7) may store vugent's other files in `.vugent/` as well, or pick another place.~~

### Scale

* The target is about 10,000 files.
* A folder with nothing changed inside it in the selected review is collapsed: it keeps its slot and size,
  but its contents are neither laid out nor drawn until the user opens it. Collapsing never moves a box.
* Layout keeps running in a Web Worker (ADR 0001). ~~Boxes with saved positions need no layout, so after the first
  visit only new boxes are placed, and a live update costs little however large the service.~~
* Svelte Flow skips boxes that are off screen. ADR 0003's warning stands: monorepos far beyond this target may need
  that choice revisited.

### Left for later

* Drawing edges between boxes, and how they combine and split across zoom levels. Edges don't affect where boxes
  are placed.
* Moving a box's saved position to its new path when git detects a rename. ~~Until then, a renamed file is a new box
  placed in free space.~~ Carried over to [ADR 0012](0012-committed-layer-layout.md).
* Services as the top level, which depends on service detection (#3).

### Implementation

| Issue | Story |
|---|---|
| #42 | ~~Save the canvas layout in `.vugent/layout.json`, keep boxes in place across live updates and reviews, Tidy and Reset~~ Read and save the committed `.vugent.yaml` ([ADR 0012](0012-committed-layer-layout.md)) |
| #43 | ~~Drag and drop boxes: top-level boxes between layers, the rest within their parent~~ Drag top-level boxes into a slot in a layer or Unsorted ([ADR 0012](0012-committed-layer-layout.md)) |
| #44 | Layers shown as rings or bands, folders as package stand-ins, ~~the folder view,~~ and the Unsorted area |
| #45 | Add, rename and reorder layers |
| #48 | Guess layers from folder names |
| #46 | Collapse unchanged folders in large repositories |
| #47 | Guess layers from known libraries and the import graph |

### Consequences

* Good, because boxes never leave their parent, so the map always shows the real structure of the software.
* Good, because the canvas becomes the user's own map of their software. A focused commit lights up one area, and
  moving between commits keeps every box where the user put it.
* ~~Good, because boxes never jump while an agent works. New boxes find free space, and removed ones leave gaps.~~
* Good, because reviewers can read top-down, bottom-up or inside-out, in the shape they think in.
* Good, because live updates and large services stay cheap: ~~saved boxes need no layout, and~~ unchanged folders
  stay collapsed.
* Good, because the feature is useful today with folders, and becomes more useful with each language plugin.
* Bad, because without a plugin the first view is only as good as the folder names. A service whose folders aren't
  named by layer starts with everything in Unsorted, and the arrangement is up to the user.
* Bad, because folders are a poor stand-in for packages in languages such as Rust, where modules are files, until
  their plugins exist.
* ~~Bad, because the layout isn't shared, so two teammates' maps of the same service differ, and a fresh clone
  starts over.~~
* ~~Bad, because over time, gaps and growth can leave the canvas untidy until the user asks for Tidy.~~

See [ADR 0012](0012-committed-layer-layout.md) for the consequences of committing the layout.
