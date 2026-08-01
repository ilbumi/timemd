import { describe, expect, it } from 'vitest';
import { clockTime, dayLabel, isoDate, shiftDays, today } from './dates';

describe('isoDate', () => {
	/**
	 * The device's own date, not UTC's: near midnight in a non-UTC zone the two
	 * disagree, and asking the server for the wrong day is exactly the bug this
	 * avoids.
	 */
	it('uses local components rather than the UTC ones', () => {
		const late = new Date(2026, 7, 1, 23, 30);
		expect(isoDate(late)).toBe('2026-08-01');
	});

	it('pads single-digit months and days', () => {
		expect(isoDate(new Date(2026, 0, 5))).toBe('2026-01-05');
	});
});

describe('shiftDays', () => {
	it('moves forwards and backwards', () => {
		expect(shiftDays('2026-08-01', 1)).toBe('2026-08-02');
		expect(shiftDays('2026-08-01', -1)).toBe('2026-07-31');
		expect(shiftDays('2026-08-01', 0)).toBe('2026-08-01');
	});

	it('crosses month and year boundaries', () => {
		expect(shiftDays('2026-08-31', 1)).toBe('2026-09-01');
		expect(shiftDays('2026-12-31', 1)).toBe('2027-01-01');
		expect(shiftDays('2026-01-01', -1)).toBe('2025-12-31');
	});

	it('handles a leap day', () => {
		expect(shiftDays('2028-02-28', 1)).toBe('2028-02-29');
	});
});

describe('clockTime', () => {
	it('trims seconds', () => {
		expect(clockTime('09:00:00')).toBe('09:00');
		expect(clockTime('09:00')).toBe('09:00');
	});
});

describe('dayLabel', () => {
	it('names the days around the reference', () => {
		expect(dayLabel('2026-08-01', '2026-08-01')).toBe('Today');
		expect(dayLabel('2026-07-31', '2026-08-01')).toBe('Yesterday');
		expect(dayLabel('2026-08-02', '2026-08-01')).toBe('Tomorrow');
	});

	it('falls back to a short date further out', () => {
		const label = dayLabel('2026-08-10', '2026-08-01');
		expect(label).not.toBe('Today');
		expect(label).toMatch(/\d/);
	});

	it('defaults its reference to the current day', () => {
		expect(dayLabel(today())).toBe('Today');
	});
});
