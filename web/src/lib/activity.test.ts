import { describe, expect, it } from 'vitest';
import {
  compare,
  fromColors,
  targets,
  type Activity,
  type Screen,
} from './activity';
import type { Color, FileNode, FolderNode, TreeNode } from './api';
import type { Box } from './layout';

function file(
  path: string,
  added: number,
  removed: number,
  color: Color,
): FileNode {
  return {
    kind: 'file',
    name: path.split('/').pop()!,
    path,
    status: 'modified',
    added,
    removed,
    binary: false,
    submodule: false,
    untracked: false,
    lines: 10,
    ...color,
  };
}

function folder(path: string, children: TreeNode[], color: Color): FolderNode {
  return {
    kind: 'folder',
    name: path,
    path,
    added: children.reduce((sum, child) => sum + child.added, 0),
    removed: children.reduce((sum, child) => sum + child.removed, 0),
    children,
    ...color,
  };
}

const white: Color = { hue: 'none' };
const lightGreen: Color = { hue: 'green', shade: 'light' };
const darkGreen: Color = { hue: 'green', shade: 'dark' };
const lightOrange: Color = { hue: 'orange', shade: 'light' };

describe('compare', () => {
  const old = folder(
    '',
    [
      folder(
        'src',
        [file('src/a.ts', 1, 0, lightGreen), file('src/b.ts', 0, 0, white)],
        lightGreen,
      ),
      file('README.md', 0, 0, white),
    ],
    lightGreen,
  );

  it('finds nothing when nothing changed', () => {
    expect(compare(old, old).size).toBe(0);
  });

  it('sparkles when the color changes and glows when only the counts do', () => {
    const next = folder(
      '',
      [
        folder(
          'src',
          [
            file('src/a.ts', 3, 0, lightGreen),
            file('src/b.ts', 2, 1, lightOrange),
          ],
          lightOrange,
        ),
        file('README.md', 0, 0, white),
        file('NEW.md', 4, 0, darkGreen),
      ],
      lightOrange,
    );

    expect(Object.fromEntries(compare(old, next))).toEqual({
      'd:src': { effect: 'dust', from: lightGreen },
      'f:src/a.ts': { effect: 'glow', from: lightGreen },
      'f:src/b.ts': { effect: 'dust', from: white },
      'f:NEW.md': { effect: 'dust' },
    });
  });

  it('counts a change of shade as a change of color', () => {
    const next = folder('', [file('README.md', 9, 0, darkGreen)], darkGreen);
    const before = folder(
      '',
      [file('README.md', 1, 0, lightGreen)],
      lightGreen,
    );

    expect(compare(before, next).get('f:README.md')?.effect).toBe('dust');
  });
});

describe('targets', () => {
  /** A folder `d:src`, open from zoom 1, holding `f:src/a.ts` and `f:src/b.ts`, and a file `f:top.ts`. */
  const boxes: Box[] = [
    {
      kind: 'folder',
      id: 'd:src',
      name: 'src',
      path: 'src',
      folder: folder('src', [], white),
      x: 0,
      y: 0,
      absX: 0,
      absY: 0,
      width: 600,
      height: 200,
      header: 20,
      revealZoom: 0,
      openZoom: 1,
    },
    ...['a', 'b'].map((name, n): Box => ({
      kind: 'file',
      id: `f:src/${name}.ts`,
      name: `${name}.ts`,
      path: `src/${name}.ts`,
      file: file(`src/${name}.ts`, 0, 0, white),
      parentId: 'd:src',
      x: 20 + n * 260,
      y: 40,
      absX: 20 + n * 260,
      absY: 40,
      width: 240,
      height: 60,
      revealZoom: 1,
    })),
    {
      kind: 'file',
      id: 'f:top.ts',
      name: 'top.ts',
      path: 'top.ts',
      file: file('top.ts', 0, 0, white),
      x: 5000,
      y: 0,
      absX: 5000,
      absY: 0,
      width: 240,
      height: 60,
      revealZoom: 0,
    },
  ];
  const screen: Screen = { x: 0, y: 0, zoom: 1, width: 1000, height: 800 };
  const dust: Activity = { effect: 'dust' };
  const glow: Activity = { effect: 'glow' };

  it('animates changed boxes that are shown', () => {
    const changed = new Map([['f:src/a.ts', dust]]);

    expect(Object.fromEntries(targets(changed, boxes, 1, screen))).toEqual({
      'f:src/a.ts': dust,
    });
  });

  it('animates the closed folder a changed file is hidden in', () => {
    const changed = new Map([
      ['f:src/a.ts', dust],
      ['f:src/b.ts', glow],
    ]);

    expect(Object.fromEntries(targets(changed, boxes, 0, screen))).toEqual({
      'd:src': glow,
    });
    changed.set('d:src', dust);
    expect(targets(changed, boxes, 0, screen).get('d:src')).toEqual(dust);
  });

  it('leaves an open folder to what it contains', () => {
    const changed = new Map([
      ['d:src', dust],
      ['f:src/b.ts', glow],
    ]);

    expect([...targets(changed, boxes, 1, screen).keys()]).toEqual([
      'f:src/b.ts',
    ]);
  });

  it('only animates boxes on screen', () => {
    const changed = new Map([
      ['f:top.ts', dust],
      ['f:src/a.ts', dust],
    ]);

    expect([...targets(changed, boxes, 1, screen).keys()]).toEqual([
      'f:src/a.ts',
    ]);
    const panned = { ...screen, x: -4500 };
    expect([...targets(changed, boxes, 1, panned).keys()]).toEqual([
      'f:top.ts',
    ]);
  });
});

describe('fromColors', () => {
  it('names the CSS colors of a box', () => {
    expect(fromColors(undefined)).toEqual({
      fill: 'transparent',
      border: 'transparent',
    });
    expect(fromColors(white)).toEqual({
      fill: 'var(--unchanged-bg)',
      border: 'var(--unchanged-border)',
    });
    expect(fromColors(darkGreen)).toEqual({
      fill: 'var(--green-dark)',
      border: 'var(--green-border)',
    });
  });
});
