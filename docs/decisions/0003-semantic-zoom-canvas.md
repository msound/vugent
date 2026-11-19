# Semantic zoom canvas with Svelte Flow

## Context and Problem Statement

Vugent shows the changes under review as an infinite canvas, like Figma or Miro, with semantic zoom (#9): what is
drawn depends on how far the user has zoomed in, not only on how large it is drawn. The deepest level is the diff of
a single file, which the user must be able to read and scroll through comfortably.

v0.1.0 deals with a single service and knows nothing about packages or imports. It shows only folders and files,
colored by how they changed in the review defined in ADR 0002. There are no edges between boxes.

ADR 0001 chose Cytoscape.js for the block diagram. Cytoscape draws on a canvas, and a node can only be a shape with
a text label. It cannot hold HTML, so a box cannot show its own contents differently as the user zooms, and later
it cannot show the HTML that server-side syntax highlighting produces.

What should be drawn at each zoom level, how does the user move between levels, and which library should draw the
canvas?

## Considered Options

Rendering library:

* Cytoscape.js (the choice in ADR 0001)
* Svelte Flow (`@xyflow/svelte`)
* A custom canvas built from DOM elements and CSS transforms, with SVG for edges
* WebGL, for example PixiJS

Layout across levels:

* One layout of the whole folder tree, with children drawn inside their parent box
* A separate layout for each level, with an animated transition from one level to the next

Diff level:

* The diff is drawn on the canvas, inside the file's box
* The diff opens in a full-screen panel with normal scrolling

## Decision Outcome

Chosen options: **Svelte Flow**, **one layout of the whole folder tree**, and **a full-screen diff panel**.

Svelte Flow is chosen because each node is a Svelte component. A node can look at the current zoom level and
decide what to show, which is what semantic zoom needs, and it is styled with plain CSS like the rest of the
frontend. It supports parent nodes for boxes inside boxes, it works with elkjs for layout, and it can draw edges
once imports are analyzed. A custom canvas would need all of this to be written by hand. WebGL would make text and
styling much harder for a scale that v0.1.0 does not need.

This replaces Cytoscape.js in the "Block diagram" row of ADR 0001. Layout still uses elkjs in a Web Worker, and the
server still sends only graph data.

One layout of the whole tree is chosen because it keeps every box in the same place at every zoom level, so the
user never loses track of where they are. A full-screen diff panel is chosen because reading a diff needs normal
scrolling, and scrolling on the canvas already means zoom.

What the user sees in v0.1.0:

| Area | Decision |
|---|---|
| Starting view | The folders and files directly inside the service root, fitted to the screen. Each folder is a box showing its name. |
| Opening a folder | A folder box shows its contents, files as smaller boxes and subfolders as boxes, once it is drawn large enough on screen. This repeats at every depth, so nested folders open one after another as the user zooms in. |
| Opening a diff | Double-clicking a file, or zooming in until the file fills most of the screen, opens its diff in a full-screen panel. The panel scrolls like a normal page. Esc closes it and returns to the canvas exactly where the user left it. So does scrolling on past the end of the diff (#21): an elastic arc rises from the bottom of the screen and snaps, closing the panel, once it is pulled far enough. Only a gesture that starts at the end pulls the arc, so a fast scroll through the diff cannot close it by accident. |
| Input | The scroll wheel, a two-finger scroll or a pinch zooms. Dragging pans. Double-clicking a folder zooms to fit it. |
| Colors | Green for added, orange for modified, red for deleted, white for unchanged. A folder is orange if anything inside it changed. Issue #4 may refine this. |
| Layout | elkjs lays out the whole tree once, with every folder sized to hold its contents. Because there are no edges, boxes are packed with ELK's rectangle packing algorithm. |
| Diff content | Plain text with added and removed lines colored. Syntax highlighting comes later; because the panel is ordinary HTML, the highlighted HTML from the server can be dropped in. |

Left for later ADRs or issues:

* Edges between boxes, and how they combine and split across zoom levels.
* Services as the top level, which depends on service detection (#3).
* Breadcrumbs, a minimap, jumping to the next change, and "fit to changes".
* Collapsing unchanged boxes, and keeping very large repositories responsive.
* Stopping one large folder from dwarfing the rest.

### Consequences

* Good, because each box decides for itself what to show at each zoom level, so new levels such as packages can be
  added without changing the canvas.
* Good, because boxes never move when the user zooms, which keeps the user oriented.
* Good, because the diff reads like a normal page and Esc always leads back to the same spot on the canvas.
* Good, because scrolling past the end of a diff is the same wheel direction that zooms out on the canvas, so the
  user leaves a diff with the reverse of the gesture that opened it, and the rest of that gesture zooms the canvas
  out.
* Good, because the frontend stays Vite + Svelte 5 and the canvas is styled with the same CSS as the rest of the UI.
* Bad, because Svelte Flow draws nodes as DOM elements, which becomes slow with many thousands of nodes. One
  service's folders and files are well within its limits, and Svelte Flow can skip nodes that are off screen.
  Large monorepos may need this decision revisited.
* Bad, because opening a diff by zooming in is a change of mode that some users may not expect. Double-click is
  offered as the obvious alternative, and the elastic arc shows plainly that scrolling on leaves the diff.
* Bad, because a very large folder makes its siblings look small at the starting zoom level.
