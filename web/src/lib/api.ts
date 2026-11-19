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
  /** Lines in the working tree. */
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

async function getJson<T>(url: string): Promise<T> {
  const response = await fetch(url);
  if (!response.ok) {
    const message = (await response.text()).trim();
    throw new Error(message || `HTTP ${response.status}`);
  }
  return response.json() as Promise<T>;
}

/** Every folder and file of the review. */
export function fetchTree(): Promise<FolderNode> {
  return getJson('/api/tree');
}

/** The diff of one file of the review. */
export function fetchDiff(path: string): Promise<FileDiff> {
  return getJson(`/api/diff?path=${encodeURIComponent(path)}`);
}
