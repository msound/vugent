// The rows of the sidebar from ADR 0005 and ADR 0006, and which one is selected.

import type { History, Rev } from './api';

export interface Row {
  rev: Rev;
  kind: 'branch' | 'worktree' | 'index' | 'unstaged' | 'commit';
  /** The branch's name, or a commit's short SHA. */
  label: string;
  /** A commit's subject, or the branch's number of commits. */
  detail?: string;
  /** Indented under the branch row. */
  indented: boolean;
}

/** Rows that show changes which are not committed yet. */
const UNCOMMITTED: Rev[] = ['worktree', 'unstaged', 'index'];

/**
 * The rows of the sidebar, top to bottom and newest first. While something is staged, uncommitted changes are split
 * into unstaged changes and then staged changes, which are one step closer to a commit. Each is only listed when
 * there are some.
 */
export function rowsOf(history: History): Row[] {
  const indented = history.branch !== null;
  const rows: Row[] = [];
  if (history.branch !== null) {
    const count = history.commits.length;
    rows.push({
      rev: 'branch',
      kind: 'branch',
      label: history.branch,
      detail: `${count} commit${count === 1 ? '' : 's'}`,
      indented: false,
    });
  }
  if (history.staged) {
    if (history.unstaged) {
      rows.push({
        rev: 'unstaged',
        kind: 'unstaged',
        label: 'Unstaged changes',
        indented,
      });
    }
    rows.push({
      rev: 'index',
      kind: 'index',
      label: 'Staged changes',
      indented,
    });
  } else if (history.uncommitted) {
    rows.push({
      rev: 'worktree',
      kind: 'worktree',
      label: 'Uncommitted changes',
      indented,
    });
  }
  for (const commit of history.commits) {
    rows.push({
      rev: commit.sha,
      kind: 'commit',
      label: commit.shortSha,
      detail: commit.subject,
      indented,
    });
  }
  return rows;
}

/**
 * The row to select: `wanted` while it is listed. Otherwise, when the page has just `opened` or `wanted` showed
 * uncommitted changes, the first row under the branch; else the newest commit, or else the first row. Undefined when
 * there are no rows.
 */
export function pick(
  rows: Row[],
  wanted: Rev | undefined,
  opened: boolean,
): Rev | undefined {
  if (wanted !== undefined && rows.some((row) => row.rev === wanted)) {
    return wanted;
  }
  const top = opened || (wanted !== undefined && UNCOMMITTED.includes(wanted));
  const fallback =
    rows.find(
      (row) => row.kind !== 'branch' && (top || row.kind === 'commit'),
    ) ?? rows[0];
  return fallback?.rev;
}
