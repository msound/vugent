// How the lines of a diff are laid out in the diff panel, from ADR 0010.

import type { Line } from './api';

/** Unified shows one column of lines; split shows old lines on the left and new lines on the right. */
export type Layout = 'unified' | 'split';

/** One row of the side-by-side layout. A side without a line shows blank cells. */
export interface SplitRow {
  left?: Line;
  right?: Line;
}

/**
 * Pairs lines for the side-by-side layout. An unchanged line appears on both sides. A run of removed lines followed
 * by a run of added lines is paired row by row, and where one run is longer the other side is left blank.
 */
export function splitRows(lines: Line[]): SplitRow[] {
  const rows: SplitRow[] = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (line.kind === 'context') {
      rows.push({ left: line, right: line });
      index += 1;
      continue;
    }
    const removed: Line[] = [];
    while (index < lines.length && lines[index].kind === 'removed') {
      removed.push(lines[index]);
      index += 1;
    }
    const added: Line[] = [];
    while (index < lines.length && lines[index].kind === 'added') {
      added.push(lines[index]);
      index += 1;
    }
    for (let row = 0; row < Math.max(removed.length, added.length); row += 1) {
      rows.push({ left: removed[row], right: added[row] });
    }
  }
  return rows;
}

/** The narrowest window, in CSS pixels, that opens diffs side by side unless the user picked a layout. */
export const SPLIT_MIN_WIDTH = 1200;

const LAYOUT_KEY = 'vugent.diffLayout';

/** The layout the user picked last, or else side-by-side on wide windows and unified on narrow ones. */
export function initialLayout(width: number): Layout {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(LAYOUT_KEY);
  } catch {
    // Storage can be blocked; the default is fine then.
  }
  if (stored === 'unified' || stored === 'split') {
    return stored;
  }
  return width >= SPLIT_MIN_WIDTH ? 'split' : 'unified';
}

/** Remembers the layout the user picked, for every diff from then on. */
export function rememberLayout(layout: Layout) {
  try {
    localStorage.setItem(LAYOUT_KEY, layout);
  } catch {
    // Not remembered, but the layout still switches.
  }
}

/** Unchanged lines this close to a change always show. */
const FOLD_CONTEXT = 3;

/** Stretches of unchanged lines this short never fold, since the row that replaces them takes as much room. */
const FOLD_MIN = 5;

/** What the diff panel shows of a hunk, in order. */
export type Block =
  | { kind: 'lines'; lines: Line[] }
  /** A stretch of unchanged lines, folded away; `key` names it across fetches of the same file. */
  | { kind: 'fold'; key: string; count: number };

/**
 * Folds the unchanged lines of a hunk that are more than three lines away from any change, unless the stretch is
 * short or its key is in `unfolded`. A hunk without changes, such as an unchanged file, is never folded.
 */
export function foldLines(
  lines: Line[],
  unfolded: (key: string) => boolean,
): Block[] {
  if (lines.every((line) => line.kind === 'context')) {
    return lines.length > 0 ? [{ kind: 'lines', lines }] : [];
  }
  // How far each line is from the nearest change, counted in lines.
  const distance = lines.map(() => Infinity);
  let last = -Infinity;
  lines.forEach((line, index) => {
    if (line.kind !== 'context') last = index;
    distance[index] = index - last;
  });
  last = Infinity;
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    if (lines[index].kind !== 'context') last = index;
    distance[index] = Math.min(distance[index], last - index);
  }

  const blocks: Block[] = [];
  const show = (shown: Line[]) => {
    const previous = blocks.at(-1);
    if (previous?.kind === 'lines') {
      previous.lines.push(...shown);
    } else {
      blocks.push({ kind: 'lines', lines: [...shown] });
    }
  };
  let index = 0;
  while (index < lines.length) {
    const start = index;
    const far = distance[index] > FOLD_CONTEXT;
    while (index < lines.length && distance[index] > FOLD_CONTEXT === far) {
      index += 1;
    }
    const stretch = lines.slice(start, index);
    const key = foldKey(stretch[0]);
    if (far && stretch.length >= FOLD_MIN && !unfolded(key)) {
      blocks.push({ kind: 'fold', key, count: stretch.length });
    } else {
      show(stretch);
    }
  }
  return blocks;
}

/**
 * Names a folded stretch by its first line in the old file, which stays put while the working tree changes, so that
 * a stretch unfolded before a live update stays unfolded after it.
 */
function foldKey(first: Line): string {
  // Only unchanged lines fold, and they have a line number on both sides.
  return String(first.oldLine);
}
