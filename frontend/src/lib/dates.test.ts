import { describe, expect, it } from 'vitest';
import {
	clockTime,
	dayLabel,
	isoDate,
	isoWeek,
	minutesOfDay,
	monthDay,
	weekDates,
	weekdayName,
	shiftDays,
	startOfWeek,
	today
} from './dates';

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

describe('week boundaries', () => {
	it('starts the week on Monday', () => {
		// 2026-08-05 is a Wednesday, 2026-08-03 the Monday before it.
		expect(startOfWeek('2026-08-05')).toBe('2026-08-03');
		expect(startOfWeek('2026-08-03')).toBe('2026-08-03');
	});

	it('treats Sunday as the end of its week, not the start', () => {
		// 2026-08-09 is a Sunday; its Monday is 2026-08-03.
		expect(startOfWeek('2026-08-09')).toBe('2026-08-03');
	});
});

describe('weekdayName and monthDay', () => {
	/** Locale-dependent, so these pin the shape rather than the spelling: the
	    point is that neither repeats what the other says. */
	it('splits the date into a weekday and a day-month', () => {
		expect(weekdayName('2026-08-01')).toMatch(/^\p{L}+$/u);
		expect(monthDay('2026-08-01')).toMatch(/1/);
		expect(monthDay('2026-08-01')).not.toBe(weekdayName('2026-08-01'));
	});

	it('reads the local date, not the UTC one', () => {
		expect(weekdayName('2026-08-01')).not.toBe(weekdayName('2026-08-02'));
	});
});

describe('weekDates', () => {
	it('gives Monday to Sunday of the week containing the date', () => {
		expect(weekDates('2026-08-01')).toEqual([
			'2026-07-27',
			'2026-07-28',
			'2026-07-29',
			'2026-07-30',
			'2026-07-31',
			'2026-08-01',
			'2026-08-02'
		]);
	});
});

describe('isoWeek', () => {
	it('numbers a mid-year week', () => {
		expect(isoWeek('2026-08-01')).toBe(31);
	});

	it('gives every day of one week the same number', () => {
		expect(['2026-07-27', '2026-07-29', '2026-08-02'].map(isoWeek)).toEqual([31, 31, 31]);
	});

	/** Week 1 is the one holding the first Thursday, so a date at the turn of the
	    year can belong to the neighbouring year's numbering. */
	it('handles the turn of the year', () => {
		expect(isoWeek('2026-01-01')).toBe(1);
		expect(isoWeek('2027-01-01')).toBe(53);
	});
});

describe('minutesOfDay', () => {
	it('counts from midnight, with or without seconds', () => {
		expect(minutesOfDay('00:00')).toBe(0);
		expect(minutesOfDay('09:30')).toBe(570);
		expect(minutesOfDay('23:59:00')).toBe(1439);
	});
});

describe('parsing guard', () => {
	it('rejects anything that is not YYYY-MM-DD', () => {
		for (const bad of ['2026-8-1', 'yesterday', '', '2026/08/01']) {
			expect(() => shiftDays(bad, 1)).toThrow(RangeError);
		}
	});
});
