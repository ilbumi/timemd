import { describe, expect, it } from 'vitest';
import { Countdown, formatClock, parseMinutes, progress } from './countdown';

describe('Countdown', () => {
	it('reports nothing before a sync', () => {
		const countdown = new Countdown();
		expect(countdown.running).toBe(false);
		expect(countdown.remaining(0)).toBe(0);
	});

	it('counts down from the synced remainder', () => {
		const countdown = new Countdown();
		countdown.sync(1500, 10_000);

		expect(countdown.remaining(10_000)).toBe(1500);
		expect(countdown.remaining(70_000)).toBe(1440);
		expect(countdown.running).toBe(true);
	});

	it('never goes negative once the deadline passes', () => {
		const countdown = new Countdown();
		countdown.sync(60, 0);

		expect(countdown.remaining(120_000)).toBe(0);
		expect(countdown.elapsed(59_000)).toBe(false);
		expect(countdown.elapsed(60_000)).toBe(true);
	});

	it('clears on a sync with nothing running', () => {
		const countdown = new Countdown();
		countdown.sync(1500, 0);
		countdown.sync(null, 1000);

		expect(countdown.running).toBe(false);
		expect(countdown.remaining(1000)).toBe(0);
		expect(countdown.elapsed(999_999)).toBe(false);
	});

	it('treats a negative remainder as already finished', () => {
		const countdown = new Countdown();
		countdown.sync(-30, 5000);

		expect(countdown.remaining(5000)).toBe(0);
		expect(countdown.elapsed(5000)).toBe(true);
	});

	/**
	 * The reason this class exists: re-syncing after the phone wakes must snap to
	 * the server's answer rather than extrapolating from a clock that stopped.
	 */
	it('snaps to the server on re-sync after a gap', () => {
		const countdown = new Countdown();
		countdown.sync(1500, 0);

		countdown.sync(300, 3_600_000);
		expect(countdown.remaining(3_600_000)).toBe(300);
	});
});

describe('formatClock', () => {
	it('formats under an hour as M:SS', () => {
		expect(formatClock(0)).toBe('0:00');
		expect(formatClock(9)).toBe('0:09');
		expect(formatClock(65)).toBe('1:05');
		expect(formatClock(1500)).toBe('25:00');
	});

	it('formats an hour and over as H:MM:SS', () => {
		expect(formatClock(3600)).toBe('1:00:00');
		expect(formatClock(3661)).toBe('1:01:01');
	});

	it('clamps negatives and truncates fractions', () => {
		expect(formatClock(-5)).toBe('0:00');
		expect(formatClock(65.9)).toBe('1:05');
	});
});

describe('progress', () => {
	it('runs from zero to one as time is spent', () => {
		expect(progress(1500, 1500)).toBe(0);
		expect(progress(750, 1500)).toBe(0.5);
		expect(progress(0, 1500)).toBe(1);
	});

	it('clamps out-of-range input', () => {
		expect(progress(-100, 1500)).toBe(1);
		expect(progress(2000, 1500)).toBe(0);
		expect(progress(10, 0)).toBe(0);
	});
});

describe('parseMinutes', () => {
	it('reads the canonical forms', () => {
		expect(parseMinutes('25m')).toBe(25);
		expect(parseMinutes('1h')).toBe(60);
		expect(parseMinutes('1h30m')).toBe(90);
		expect(parseMinutes('0m')).toBe(0);
	});

	/** Matches core's `Minutes`, which rejects an unlabelled number. */
	it('returns zero for anything the server would not have written', () => {
		for (const bad of ['', '90', 'ages', '1h30', '1.5h']) {
			expect(parseMinutes(bad)).toBe(0);
		}
	});
});
