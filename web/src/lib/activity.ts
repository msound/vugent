// Which boxes show activity when a review is fetched again (ADR 0008): fairy dust when a box's color changes, a soft
// glow when only its line counts do.

import type { Color, FolderNode, TreeNode } from './api';
import type { Box } from './layout';

/** `dust` when the hue or shade changed, or the box is new; `glow` when only the line counts changed. */
export type Effect = 'dust' | 'glow';

/** How a box changed: its effect, and its color before, or none for a new box. */
export interface Activity {
  effect: Effect;
  from?: Color;
}

/** Where the canvas is looking, in screen pixels: the viewport's offset and zoom, and the screen's size. */
export interface Screen {
  x: number;
  y: number;
  zoom: number;
  width: number;
  height: number;
}

/** The box id of a tree node, as the layout names it. */
function idOf(node: TreeNode): string {
  return `${node.kind === 'folder' ? 'd' : 'f'}:${node.path}`;
}

/** Every folder and file under `root`, not including `root` itself, by box id. */
function index(root: FolderNode): Map<string, TreeNode> {
  const nodes = new Map<string, TreeNode>();
  const visit = (folder: FolderNode) => {
    for (const child of folder.children) {
      nodes.set(idOf(child), child);
      if (child.kind === 'folder') {
        visit(child);
      }
    }
  };
  visit(root);
  return nodes;
}

const sameColor = (a: Color, b: Color) =>
  a.hue === b.hue && (a.shade ?? null) === (b.shade ?? null);

/**
 * The boxes whose line counts changed from `old` to `next`, by box id. New files count as changed; boxes that are
 * gone have nothing left to animate.
 */
export function compare(
  old: FolderNode,
  next: FolderNode,
): Map<string, Activity> {
  const before = index(old);
  const changed = new Map<string, Activity>();
  for (const [id, node] of index(next)) {
    const was = before.get(id);
    if (!was) {
      // A new folder is shown by the new files in it.
      if (node.kind === 'file') {
        changed.set(id, { effect: 'dust' });
      }
    } else if (!sameColor(was, node)) {
      changed.set(id, {
        effect: 'dust',
        from: { hue: was.hue, shade: was.shade },
      });
    } else if (was.added !== node.added || was.removed !== node.removed) {
      changed.set(id, {
        effect: 'glow',
        from: { hue: was.hue, shade: was.shade },
      });
    }
  }
  return changed;
}

/**
 * The boxes to animate for `changed`, by box id: a changed box that is shown, or the closed folder a changed box is
 * hidden in, so that activity is never hidden. A folder that is open shows its activity on what it contains instead.
 * Only boxes on `screen` animate; `reached` is the zoom threshold that decides which boxes are shown and open.
 */
export function targets(
  changed: Map<string, Activity>,
  boxes: Box[],
  reached: number,
  screen: Screen,
): Map<string, Activity> {
  const byId = new Map(boxes.map((box) => [box.id, box]));
  const result = new Map<string, Activity>();
  for (const id of changed.keys()) {
    let box = byId.get(id);
    while (box && box.revealZoom > reached) {
      box = box.parentId === undefined ? undefined : byId.get(box.parentId);
    }
    if (
      !box ||
      (box.kind === 'folder' && box.openZoom <= reached) ||
      !onScreen(box, screen)
    ) {
      continue;
    }
    // A closed folder shows how it changed itself; it may have kept its color while a file in it did not.
    const activity = changed.get(box.id) ?? { effect: 'glow' };
    if (result.get(box.id)?.effect !== 'dust') {
      result.set(box.id, activity);
    }
  }
  return result;
}

function onScreen(box: Box, { x, y, zoom, width, height }: Screen): boolean {
  const left = box.absX * zoom + x;
  const top = box.absY * zoom + y;
  return (
    left < width &&
    top < height &&
    left + box.width * zoom > 0 &&
    top + box.height * zoom > 0
  );
}

/** The CSS colors a box had, to fade from: transparent for a new box. */
export function fromColors(from: Color | undefined): {
  fill: string;
  border: string;
} {
  if (!from) {
    return { fill: 'transparent', border: 'transparent' };
  }
  if (from.hue === 'none') {
    return {
      fill: 'var(--unchanged-bg)',
      border: 'var(--unchanged-border)',
    };
  }
  return {
    fill: `var(--${from.hue}-${from.shade ?? 'light'})`,
    border: `var(--${from.hue}-border)`,
  };
}
