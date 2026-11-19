// What a live update from ADR 0008 affects, and running fetches one at a time.

import type { FileNode, FolderNode, Rev } from './api';

/** A change the server reports on `/api/events`. */
export type Change =
  /** Files in the reviewed folder changed; `paths` is null when which ones is not known. */
  | { kind: 'files'; paths: string[] | null }
  /** The index changed, such as after `git add`. */
  | { kind: 'index' }
  /** `HEAD` or a ref changed: a new commit, a branch switch, a rebase, a reset. */
  | { kind: 'refs' };

/** The reviews that are not single commits. */
const LIVE: Rev[] = ['worktree', 'index', 'unstaged', 'branch'];

/** Which reviews each kind of change affects, from the table in ADR 0008. Refs affect every live review. */
const AFFECTED: Record<Change['kind'], Rev[]> = {
  files: ['worktree', 'unstaged', 'branch'],
  index: ['index', 'unstaged', 'worktree'],
  refs: LIVE,
};

/** Whether `change` may have changed the review `rev`. A commit's review never changes. */
export function affects(change: Change, rev: Rev): boolean {
  return AFFECTED[change.kind].includes(rev);
}

/**
 * Whether `change` may have changed the diff of the file at `path` in the review `rev`: the review is affected, and
 * the change names the file or a folder it is in, or names no paths.
 */
export function touches(change: Change, rev: Rev, path: string): boolean {
  if (!affects(change, rev)) {
    return false;
  }
  if (change.kind !== 'files' || change.paths === null) {
    return true;
  }
  return change.paths.some(
    (changed) => changed === path || path.startsWith(`${changed}/`),
  );
}

/** The file at `path` in `tree`, if there is one. */
export function findFile(tree: FolderNode, path: string): FileNode | undefined {
  for (const child of tree.children) {
    if (child.kind === 'file') {
      if (child.path === path) {
        return child;
      }
    } else if (path.startsWith(`${child.path}/`)) {
      return findFile(child, path);
    }
  }
  return undefined;
}

/**
 * Wraps `task` so that it runs one at a time: calling it while it runs runs it once more afterwards, however many
 * times it was called meanwhile, so that what it fetches is never older than the last call.
 */
export function serial(task: () => Promise<void>): () => Promise<void> {
  let running: Promise<void> | undefined;
  let again = false;
  async function loop() {
    do {
      again = false;
      try {
        await task();
      } catch {
        // The task reports its own failures; a failed run must not stop the next one.
      }
    } while (again);
    running = undefined;
  }
  return () => {
    if (running) {
      again = true;
    } else {
      running = loop();
    }
    return running;
  };
}
