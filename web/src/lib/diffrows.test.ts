import { describe, expect, it } from 'vitest';
import type { Line } from './api';
import { splitRows } from './diffrows';

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
