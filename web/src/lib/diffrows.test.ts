import { describe, expect, it } from 'vitest';
import type { Line } from './api';
import { foldLines, piecesOf, splitRows } from './diffrows';

const context = (old: number, now: number, text = 'x'): Line => ({
  kind: 'context',
  oldLine: old,
  newLine: now,
  text,
});
const removed = (old: number, text = '-'): Line => ({
  kind: 'removed',
  oldLine: old,
  text,
});
const added = (now: number, text = '+'): Line => ({
  kind: 'added',
  newLine: now,
  text,
});

/** Each row as its left and right line numbers, with null for a blank side. */
const numbers = (lines: Line[]) =>
  splitRows(lines).map(({ left, right }) => [
    left?.oldLine ?? null,
    right?.newLine ?? null,
  ]);

describe('splitRows', () => {
  it('shows unchanged lines on both sides', () => {
    expect(numbers([context(1, 1), context(2, 3)])).toEqual([
      [1, 1],
      [2, 3],
    ]);
  });

  it('pairs removed lines with the added lines after them', () => {
    expect(
      numbers([
        context(1, 1),
        removed(2),
        removed(3),
        added(2),
        added(3),
        added(4),
        context(4, 5),
      ]),
    ).toEqual([
      [1, 1],
      [2, 2],
      [3, 3],
      [null, 4],
      [4, 5],
    ]);
  });

  it('leaves the other side blank when lines are only added or only removed', () => {
    expect(
      numbers([added(1), context(1, 2), removed(2), removed(3), context(4, 3)]),
    ).toEqual([
      [null, 1],
      [1, 2],
      [2, null],
      [3, null],
      [4, 3],
    ]);
  });

  it('does not pair added lines with removed lines after them', () => {
    expect(numbers([added(1), removed(1)])).toEqual([
      [null, 1],
      [1, null],
    ]);
  });
});

describe('foldLines', () => {
  /** `count` unchanged lines starting at line `from` on both sides. */
  const unchanged = (from: number, count: number) =>
    Array.from({ length: count }, (_, index) =>
      context(from + index, from + index),
    );
  const none = () => false;
  const shape = (blocks: ReturnType<typeof foldLines>) =>
    blocks.map((block) =>
      block.kind === 'fold' ? `fold ${block.count}` : `${block.lines.length}`,
    );

  it('folds unchanged lines more than three lines away from a change', () => {
    const lines = [
      ...unchanged(1, 10),
      removed(11),
      added(11),
      ...unchanged(12, 20),
    ];

    expect(shape(foldLines(lines, none))).toEqual(['fold 7', '8', 'fold 17']);
  });

  it('never folds four lines or fewer', () => {
    const lines = [...unchanged(1, 7), added(8), ...unchanged(8, 7)];

    expect(shape(foldLines(lines, none))).toEqual(['15']);
  });

  it('folds between changes', () => {
    const lines = [added(1), ...unchanged(1, 12), removed(13)];

    expect(shape(foldLines(lines, none))).toEqual(['4', 'fold 6', '4']);
  });

  it('keeps unfolded stretches open by their first old line', () => {
    const lines = [...unchanged(1, 10), added(11), ...unchanged(11, 10)];

    const blocks = foldLines(lines, (key) => key === '1');

    expect(shape(blocks)).toEqual(['14', 'fold 7']);
    expect(blocks[1]).toMatchObject({ key: '14' });
  });

  it('does not fold a file without changes', () => {
    expect(shape(foldLines(unchanged(1, 50), none))).toEqual(['50']);
    expect(foldLines([], none)).toEqual([]);
  });
});

describe('piecesOf', () => {
  const oldLines = [[{ text: 'a', class: 'keyword' }], [{ text: '-' }]];
  const newLines = [[{ text: 'x' }], [{ text: '+', class: 'string' }]];

  it('colors removed lines from the old file and the others from the new file', () => {
    expect(piecesOf(removed(2), oldLines, newLines)).toBe(oldLines[1]);
    expect(piecesOf(added(2), oldLines, newLines)).toBe(newLines[1]);
    expect(piecesOf(context(1, 1), oldLines, newLines)).toBe(newLines[0]);
  });

  it('leaves lines plain without highlighting or when it does not match', () => {
    expect(piecesOf(removed(2), undefined, newLines)).toBeUndefined();
    expect(piecesOf(added(3), oldLines, newLines)).toBeUndefined();
    expect(piecesOf(added(1), oldLines, newLines)).toBeUndefined();
  });

  it('highlights empty lines', () => {
    expect(piecesOf(added(1, ''), oldLines, [[]])).toEqual([]);
  });
});
