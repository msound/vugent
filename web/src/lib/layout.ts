// Lays out the whole folder tree once with elkjs, in a Web Worker, as ADR 0003 describes: every folder is a box
// sized to hold its contents, packed with ELK's rectangle packing algorithm.

import ELK, { type ElkNode } from 'elkjs/lib/elk-api';
import elkWorkerUrl from 'elkjs/lib/elk-worker.min.js?url';
import type { FileNode, FolderNode, Status, TreeNode } from './api';

/** Every file is drawn the same size, in canvas units. */
export const FILE_WIDTH = 240;
export const FILE_HEIGHT = 60;

/** A folder shows its contents once its smaller side is drawn at least this many pixels on screen. */
export const OPEN_PIXELS = 200;

interface BoxBase {
  /** Unique among boxes: `d:` or `f:` followed by the path. */
  id: string;
  name: string;
  path: string;
  /** The id of the folder box this box is drawn in, if any. */
  parentId?: string;
  /** Position relative to the parent box. */
  x: number;
  y: number;
  /** Position on the canvas. */
  absX: number;
  absY: number;
  width: number;
  height: number;
  /** The smallest zoom at which every folder around this box is open, so that the box is shown. */
  revealZoom: number;
}

export interface FolderBox extends BoxBase {
  kind: 'folder';
  /** A folder is orange (modified) if anything inside it changed. */
  status: 'modified' | 'unchanged';
  /** Height of the band at the top that holds the folder's name. */
  header: number;
  /** The smallest zoom at which the folder shows its contents. */
  openZoom: number;
}

export interface FileBox extends BoxBase {
  kind: 'file';
  status: Status;
  file: FileNode;
}

export type Box = FolderBox | FileBox;

interface FolderSpacing {
  header: number;
  padding: number;
}

let elk: InstanceType<typeof ELK> | undefined;

/** Lays out every folder and file under `root`, not including `root` itself. Parents come before children. */
export async function layout(root: FolderNode): Promise<Box[]> {
  elk ??= new ELK({ workerUrl: elkWorkerUrl });

  const spacing = new Map<string, FolderSpacing>();
  estimateArea(root, spacing);
  const graph: ElkNode = {
    id: 'root',
    layoutOptions: packing({ header: 0, padding: 0 }, rootGap(root, spacing)),
    children: root.children.map((child) => elkNode(child, spacing)),
  };
  const result = await elk.layout(graph);

  const byId = new Map<string, TreeNode>();
  index(root, byId);
  const boxes: Box[] = [];
  for (const child of result.children ?? []) {
    collect(child, undefined, 0, 0, 0, byId, spacing, boxes);
  }
  return boxes;
}

function id(node: TreeNode): string {
  return `${node.kind === 'folder' ? 'd' : 'f'}:${node.path}`;
}

function index(node: TreeNode, byId: Map<string, TreeNode>) {
  byId.set(id(node), node);
  if (node.kind === 'folder') {
    node.children.forEach((child) => index(child, byId));
  }
}

/**
 * Estimates the area of every folder before layout, so that the band holding a folder's name, and the gaps
 * around its contents, can grow with the folder. Then a folder's name is as readable on screen as its contents when
 * it opens, whatever its size.
 */
function estimateArea(
  node: TreeNode,
  spacing: Map<string, FolderSpacing>,
): number {
  if (node.kind === 'file') {
    return FILE_WIDTH * FILE_HEIGHT;
  }
  // Packing leaves some space unused.
  const content =
    1.3 *
    node.children.reduce((sum, child) => sum + estimateArea(child, spacing), 0);
  const side = Math.sqrt(content);
  const header = Math.max(FILE_HEIGHT * 0.6, side * 0.05);
  const padding = Math.max(FILE_HEIGHT * 0.2, header * 0.3);
  spacing.set(id(node), { header, padding });
  return (side + 2 * padding) * (side + padding + header);
}

function rootGap(
  root: FolderNode,
  spacing: Map<string, FolderSpacing>,
): number {
  return spacing.get(id(root))?.padding ?? FILE_HEIGHT * 0.2;
}

function packing(space: FolderSpacing, gap: number): Record<string, string> {
  return {
    'elk.algorithm': 'rectpacking',
    'elk.aspectRatio': '1.6',
    'elk.spacing.nodeNode': String(gap),
    'elk.padding': `[top=${space.header},left=${space.padding},bottom=${space.padding},right=${space.padding}]`,
  };
}

function elkNode(node: TreeNode, spacing: Map<string, FolderSpacing>): ElkNode {
  if (node.kind === 'file') {
    return { id: id(node), width: FILE_WIDTH, height: FILE_HEIGHT };
  }
  const space = spacing.get(id(node))!;
  return {
    id: id(node),
    layoutOptions: packing(space, space.padding),
    children: node.children.map((child) => elkNode(child, spacing)),
  };
}

/** Turns the laid out ELK graph into boxes, parents first. Returns whether anything in the subtree changed. */
function collect(
  elkBox: ElkNode,
  parent: FolderBox | undefined,
  parentX: number,
  parentY: number,
  revealZoom: number,
  byId: Map<string, TreeNode>,
  spacing: Map<string, FolderSpacing>,
  boxes: Box[],
): boolean {
  const node = byId.get(elkBox.id)!;
  const base = {
    id: elkBox.id,
    name: node.name,
    path: node.path,
    parentId: parent?.id,
    x: elkBox.x ?? 0,
    y: elkBox.y ?? 0,
    absX: parentX + (elkBox.x ?? 0),
    absY: parentY + (elkBox.y ?? 0),
    width: elkBox.width ?? FILE_WIDTH,
    height: elkBox.height ?? FILE_HEIGHT,
    revealZoom,
  };
  if (node.kind === 'file') {
    boxes.push({ ...base, kind: 'file', status: node.status, file: node });
    return node.status !== 'unchanged';
  }

  const openZoom = Math.max(
    revealZoom,
    OPEN_PIXELS / Math.min(base.width, base.height),
  );
  const folder: FolderBox = {
    ...base,
    kind: 'folder',
    status: 'unchanged',
    header: spacing.get(elkBox.id)!.header,
    openZoom,
  };
  boxes.push(folder);
  let changed = false;
  for (const child of elkBox.children ?? []) {
    const childChanged = collect(
      child,
      folder,
      base.absX,
      base.absY,
      openZoom,
      byId,
      spacing,
      boxes,
    );
    changed ||= childChanged;
  }
  folder.status = changed ? 'modified' : 'unchanged';
  return changed;
}
