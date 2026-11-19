import { describe, expect, it } from 'vitest';
import type { FileNode, FolderNode } from './api';
import { affects, findFile, serial, touches, type Change } from './live';

const SHA = 'a'.repeat(40);

const files = (...paths: string[]): Change => ({ kind: 'files', paths });
const unknownFiles: Change = { kind: 'files', paths: null };
const index: Change = { kind: 'index' };
const refs: Change = { kind: 'refs' };

describe('affects', () => {
  it('follows the table of ADR 0008', () => {
    const affected = (change: Change) =>
      ['worktree', 'index', 'unstaged', 'branch', SHA].filter((rev) =>
        affects(change, rev),
      );

    expect(affected(files('a.txt'))).toEqual([
      'worktree',
      'unstaged',
      'branch',
    ]);
    expect(affected(index)).toEqual(['worktree', 'index', 'unstaged']);
    expect(affected(refs)).toEqual(['worktree', 'index', 'unstaged', 'branch']);
  });
});

describe('touches', () => {
  it('matches the file or a folder it is in', () => {
    expect(touches(files('src/a.ts'), 'worktree', 'src/a.ts')).toBe(true);
    expect(touches(files('src'), 'worktree', 'src/a.ts')).toBe(true);
    expect(touches(files('src/a'), 'worktree', 'src/a.ts')).toBe(false);
    expect(touches(files('b.ts'), 'worktree', 'src/a.ts')).toBe(false);
  });

  it('matches every file when no paths are known', () => {
    expect(touches(unknownFiles, 'branch', 'src/a.ts')).toBe(true);
    expect(touches(index, 'index', 'src/a.ts')).toBe(true);
    expect(touches(refs, 'branch', 'src/a.ts')).toBe(true);
  });

  it('never matches a review the change does not affect', () => {
    expect(touches(files('src/a.ts'), 'index', 'src/a.ts')).toBe(false);
    expect(touches(refs, SHA, 'src/a.ts')).toBe(false);
  });
});

describe('findFile', () => {
  const file = (path: string): FileNode => ({
    kind: 'file',
    name: path.split('/').pop()!,
    path,
    status: 'modified',
    added: 1,
    removed: 0,
    binary: false,
    submodule: false,
    untracked: false,
    lines: 1,
    hue: 'green',
    shade: 'light',
  });
  const folder = (
    path: string,
    children: FolderNode['children'],
  ): FolderNode => ({
    kind: 'folder',
    name: path,
    path,
    added: 0,
    removed: 0,
    hue: 'none',
    children,
  });
  const tree = folder('', [
    folder('src', [
      folder('src/lib', [file('src/lib/a.ts')]),
      file('src/b.ts'),
    ]),
    folder('srcx', [file('srcx/c.ts')]),
    file('README.md'),
  ]);

  it('finds files at any depth', () => {
    expect(findFile(tree, 'src/lib/a.ts')?.path).toBe('src/lib/a.ts');
    expect(findFile(tree, 'srcx/c.ts')?.path).toBe('srcx/c.ts');
    expect(findFile(tree, 'README.md')?.path).toBe('README.md');
  });

  it('finds nothing for paths that are not files', () => {
    expect(findFile(tree, 'src/lib')).toBeUndefined();
    expect(findFile(tree, 'nope.txt')).toBeUndefined();
  });
});

describe('serial', () => {
  /** A task whose runs finish when the test says so. */
  function controlled() {
    const finish: (() => void)[] = [];
    let runs = 0;
    const task = () => {
      runs += 1;
      return new Promise<void>((resolve) => finish.push(resolve));
    };
    return { task, finish, runs: () => runs };
  }

  it('runs once more after calls made while it runs', async () => {
    const { task, finish, runs } = controlled();
    const run = serial(task);

    const first = run();
    void run();
    void run();
    expect(runs()).toBe(1);

    finish.shift()!();
    await Promise.resolve();
    await Promise.resolve();
    expect(runs()).toBe(2);

    finish.shift()!();
    await first;
    expect(runs()).toBe(2);
  });

  it('keeps going after a failure', async () => {
    let runs = 0;
    const run = serial(async () => {
      runs += 1;
      throw new Error('offline');
    });

    await run();
    await run();

    expect(runs).toBe(2);
  });
});
