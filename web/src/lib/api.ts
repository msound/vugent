// Types and requests for the vugent server's API.

export type Status = 'added' | 'modified' | 'deleted' | 'renamed' | 'unchanged';

/** What kind of change a box shows, from ADR 0004: none, only added, only removed, or both. */
export type Hue = 'none' | 'green' | 'red' | 'orange';

/** How much of a box changed: under half, at least half, or all of it. */
export type Shade = 'light' | 'medium' | 'dark';

/** How a box is colored; `shade` is absent when the hue is `none`. */
export interface Color {
  hue: Hue;
  shade?: Shade;
}

export interface FolderNode extends Color {
  kind: 'folder';
  name: string;
  /** Relative to the reviewed folder; empty for the reviewed folder itself. */
  path: string;
  /** Lines added in every file inside the folder. */
  added: number;
  /** Lines removed in every file inside the folder. */
  removed: number;
  children: TreeNode[];
}

export interface FileNode extends Color {
  kind: 'file';
  name: string;
  path: string;
  status: Status;
  added: number;
  removed: number;
  binary: boolean;
  oldPath?: string;
  submodule: boolean;
  untracked: boolean;
  /** Lines in the target of the review. */
  lines: number;
}

export type TreeNode = FolderNode | FileNode;

export type LineKind = 'added' | 'removed' | 'context';

export interface Line {
  kind: LineKind;
  oldLine?: number;
  newLine?: number;
  text: string;
}

export interface Hunk {
  header: string;
  oldStart: number;
  newStart: number;
  lines: Line[];
}

export interface FileDiff {
  path: string;
  oldPath?: string;
  status: Status;
  binary: boolean;
  submodule: boolean;
  hunks: Hunk[];
}

export interface Commit {
  sha: string;
  shortSha: string;
  /** The first line of the commit message. */
  subject: string;
  /** The author's name. */
  author: string;
  /** When it was authored, in strict ISO 8601. */
  date: string;
}

/** Where the branch left the main branch. */
export interface Base {
  /** The main branch, such as `main` or `origin/main`. */
  name: string;
  /** The short SHA of the merge-base. */
  shortSha: string;
}

/** What can be reviewed, from ADR 0005, ADR 0006 and ADR 0007. */
export interface History {
  /** The branch's name, or null when there is no branch review. */
  branch: string | null;
  /** What the branch is compared against; null exactly when `branch` is. */
  base: Base | null;
  /** Something in the reviewed folder is uncommitted. */
  uncommitted: boolean;
  /** Something in the reviewed folder is staged. */
  staged: boolean;
  /** Something in the reviewed folder differs from the index, or is untracked. */
  unstaged: boolean;
  /** Newest first: the branch's commits, or recent ones when there is no branch. */
  commits: Commit[];
  /** The repository's web address when `origin` is on GitHub, such as `https://github.com/owner/repo`. */
  github: string | null;
}

/** Who wrote or committed a commit, and when. */
export interface Signature {
  name: string;
  email: string;
  /** Strict ISO 8601, with the committer's offset from UTC. */
  date: string;
}

/** A trailer at the end of a commit message, such as `Closes: #25`. */
export interface Trailer {
  key: string;
  value: string;
}

/** Everything the detail page of ADR 0007 shows about a commit. */
export interface CommitDetails {
  sha: string;
  shortSha: string;
  subject: string;
  /** The message after the subject, without the trailers. */
  body: string;
  trailers: Trailer[];
  author: Signature;
  committer: Signature;
}

/**
 * Which review to show: `worktree` (uncommitted changes), `index` (staged changes), `unstaged` (unstaged changes),
 * `branch`, or a commit's full SHA.
 */
export type Rev = string;

async function getJson<T>(url: string): Promise<T> {
  const response = await fetch(url);
  if (!response.ok) {
    const message = (await response.text()).trim();
    throw new Error(message || `HTTP ${response.status}`);
  }
  return response.json() as Promise<T>;
}

/** What can be reviewed, for the sidebar. */
export function fetchHistory(): Promise<History> {
  return getJson('/api/commits');
}

/** Every folder and file of the review `rev`. */
export function fetchTree(rev: Rev): Promise<FolderNode> {
  return getJson(`/api/tree?rev=${encodeURIComponent(rev)}`);
}

/** The diff of one file of the review `rev`. */
export function fetchDiff(path: string, rev: Rev): Promise<FileDiff> {
  const query = new URLSearchParams({ path, rev });
  return getJson(`/api/diff?${query}`);
}

/** A commit's message, trailers, author and committer, by its full SHA. */
export function fetchCommit(sha: string): Promise<CommitDetails> {
  return getJson(`/api/commit?rev=${encodeURIComponent(sha)}`);
}
