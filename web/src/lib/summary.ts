// What the header and the detail page of ADR 0007 say about a review as a whole.

import type { FileNode, FolderNode, TreeNode } from './api';

export interface Summary {
  /** Lines added in every file. */
  added: number;
  /** Lines removed in every file. */
  removed: number;
  /** The files that changed, by path. */
  files: FileNode[];
}

/** Sums up the review whose tree is `tree`. */
export function summarize(tree: FolderNode): Summary {
  const files: FileNode[] = [];
  const visit = (node: TreeNode) => {
    if (node.kind === 'folder') {
      node.children.forEach(visit);
    } else if (node.status !== 'unchanged') {
      files.push(node);
    }
  };
  visit(tree);
  files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return { added: tree.added, removed: tree.removed, files };
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
];

const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

/** How long ago `date` was, such as "3 hours ago", or "just now" under a minute. */
export function relativeTime(date: string, now: number = Date.now()): string {
  const seconds = (new Date(date).getTime() - now) / 1000;
  for (const [unit, size] of UNITS) {
    if (Math.abs(seconds) >= size) {
      return relative.format(Math.round(seconds / size), unit);
    }
  }
  return 'just now';
}

/** `count` and the noun, made plural unless it is one: "1 file", "3 files". */
export function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? '' : 's'}`;
}
