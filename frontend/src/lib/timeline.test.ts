import { describe, expect, it } from 'vitest';
import { hourMarks, minutesNow, offsetIn, placeIn, spanOf } from './timeline';

const block = (start: string, end: string) => ({ start, end });

describe('spanOf', () => {
	it('uses the default hours when everything fits inside them', () => {
		expect(spanOf([block('09:00', '11:00')], 480, 1200)).toEqual({ from: 480, to: 1200 });
	});

	/** The reason this is shared: a block outside the default hours has to stay
	    on screen, and both views get that wrong in the same way if they diverge. */
	it('stretches to hold a block that starts early or ends late', () => {
		expect(spanOf([block('06:30', '23:30')], 480, 1200)).toEqual({ from: 390, to: 1410 });
	});

	it('keeps an hour of height when there is nothing to draw', () => {
		expect(spanOf([], 600, 600)).toEqual({ from: 600, to: 660 });
	});
});

describe('hourMarks', () => {
	it('labels every step-th hour inside the span', () => {
		expect(hourMarks({ from: 480, to: 1200 }, 2)).toEqual([8, 10, 12, 14, 16, 18, 20]);
		expect(hourMarks({ from: 480, to: 1200 }, 3)).toEqual([8, 11, 14, 17, 20]);
	});

	it('starts at the first whole hour inside a span that begins mid-hour', () => {
		expect(hourMarks({ from: 390, to: 600 }, 2)).toEqual([7, 9]);
	});
});

describe('offsetIn', () => {
	it('runs from zero at the top to a hundred at the bottom', () => {
		const span = { from: 480, to: 1200 };
		expect(offsetIn(span, 480)).toBe(0);
		expect(offsetIn(span, 1200)).toBe(100);
		expect(offsetIn(span, 840)).toBe(50);
	});
});

describe('placeIn', () => {
	it('gives a block its top and its height', () => {
		const place = placeIn({ from: 480, to: 1200 }, block('09:00', '11:00'));
		// Percentages of a 12-hour window: an hour in, two hours long.
		expect(place.top).toBeCloseTo(100 / 12);
		expect(place.height).toBeCloseTo(200 / 12);
	});
});

describe('minutesNow', () => {
	it('counts the local wall clock from midnight', () => {
		expect(minutesNow(new Date(2026, 7, 1, 9, 30))).toBe(570);
		expect(minutesNow(new Date(2026, 7, 1, 0, 0))).toBe(0);
	});
});
