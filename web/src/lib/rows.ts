// The rows of the sidebar from ADR 0005, and which one is selected.

import type { History, Rev } from './api';

export interface Row {
  rev: Rev;
  kind: 'branch' | 'worktree' | 'index' | 'commit';
  /** The branch's name, or a commit's short SHA. */
  label: string;
  /** A commit's subject, or the branch's number of commits. */
  detail?: string;
  /** Indented under the branch row. */
  indented: boolean;
}

/** The rows of the sidebar, top to bottom. Uncommitted and staged changes are only listed when there are some. */
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
  if (history.uncommitted) {
    rows.push({
      rev: 'worktree',
      kind: 'worktree',
      label: 'Uncommitted changes',
      indented,
    });
  }
  if (history.staged) {
    rows.push({
      rev: 'index',
      kind: 'index',
      label: 'Staged changes',
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
 * The row to select: `wanted` while it is listed. Otherwise, when the page has just `opened`, uncommitted changes,
 * and in any case the newest commit, or else the first row. Undefined when there are no rows.
 */
export function pick(
  rows: Row[],
  wanted: Rev | undefined,
  opened: boolean,
): Rev | undefined {
  if (wanted !== undefined && rows.some((row) => row.rev === wanted)) {
    return wanted;
  }
  const fallback =
    (opened ? rows.find((row) => row.kind === 'worktree') : undefined) ??
    rows.find((row) => row.kind === 'commit') ??
    rows[0];
  return fallback?.rev;
}
