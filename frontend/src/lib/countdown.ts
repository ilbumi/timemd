/**
 * Countdown arithmetic, kept out of the component so it can be tested.
 *
 * The server owns the deadline, but the phone has to render a smooth second
 * hand between polls. Doing that from the device's wall clock would drift
 * whenever the two disagree — and on a phone that has been asleep, they will.
 * So a poll fixes the remaining time against `performance.now()`, a monotonic
 * source, and ticking is pure subtraction from there.
 */

/** Tracks the deadline of the running session in monotonic time. */
export class Countdown {
	private deadline: number | null = null;

	/**
	 * Anchors to a fresh reading from the server.
	 *
	 * @param remainingSeconds seconds left, or `null` when nothing is running
	 * @param monotonicNow a `performance.now()` reading taken alongside it
	 */
	sync(remainingSeconds: number | null, monotonicNow: number): void {
		this.deadline =
			remainingSeconds === null ? null : monotonicNow + Math.max(0, remainingSeconds) * 1000;
	}

	/** Whole seconds left, never negative. Zero once nothing is running. */
	remaining(monotonicNow: number): number {
		if (this.deadline === null) return 0;
		return Math.max(0, Math.round((this.deadline - monotonicNow) / 1000));
	}

	get running(): boolean {
		return this.deadline !== null;
	}

	/** True once the deadline has passed, which is when the client re-polls. */
	elapsed(monotonicNow: number): boolean {
		return this.deadline !== null && monotonicNow >= this.deadline;
	}
}

/** Seconds as `M:SS`, or `H:MM:SS` past an hour. */
export function formatClock(totalSeconds: number): string {
	const safe = Math.max(0, Math.floor(totalSeconds));
	const hours = Math.floor(safe / 3600);
	const minutes = Math.floor((safe % 3600) / 60);
	const seconds = safe % 60;

	const padded = `${minutes.toString().padStart(hours > 0 ? 2 : 1, '0')}:${seconds
		.toString()
		.padStart(2, '0')}`;

	return hours > 0 ? `${hours}:${padded}` : padded;
}

/** Fraction of a session already spent, clamped to 0..1, for the dial. */
export function progress(remainingSeconds: number, totalSeconds: number): number {
	if (totalSeconds <= 0) return 0;
	return Math.min(1, Math.max(0, 1 - remainingSeconds / totalSeconds));
}
