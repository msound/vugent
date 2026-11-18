// Types and requests for the vugent server's API.

export type Status = 'added' | 'modified' | 'deleted' | 'renamed' | 'unchanged';

export interface FolderNode {
  kind: 'folder';
  name: string;
  /** Relative to the reviewed folder; empty for the reviewed folder itself. */
  path: string;
  children: TreeNode[];
}

export interface FileNode {
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
