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
