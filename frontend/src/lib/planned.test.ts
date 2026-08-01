import { describe, expect, it } from 'vitest';
import type { DayView, Occurrence } from './api';
import { oneOffIndex } from './planned';

function occurrence(title: string, block: string | null): Occurrence {
	return {
		date: '2026-08-05',
		start: '09:00:00',
		end: '10:00:00',
		duration: '1h',
		project: null,
		title,
		remindBefore: null,
		block
	};
}

function dayWith(planned: Occurrence[]): DayView {
	return { date: '2026-08-05', sessions: [], tracked: '0m', planned, skipped: [], problems: [] };
}

describe('oneOffIndex', () => {
	it('is the same number when every block is a one-off', () => {
		const day = dayWith([occurrence('a', null), occurrence('b', null)]);
		expect(oneOffIndex(day, 0)).toBe(0);
		expect(oneOffIndex(day, 1)).toBe(1);
	});

	it('skips repeats that sort earlier', () => {
		const day = dayWith([
			occurrence('repeat', 'deep-work'),
			occurrence('lunch', null),
			occurrence('repeat two', 'review'),
			occurrence('walk', null)
		]);

		expect(oneOffIndex(day, 1)).toBe(0);
		expect(oneOffIndex(day, 3)).toBe(1);
	});

	it('is zero for an empty day', () => {
		expect(oneOffIndex(dayWith([]), 0)).toBe(0);
	});
});
