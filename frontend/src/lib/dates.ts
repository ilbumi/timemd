/** Local-date helpers. The API speaks `YYYY-MM-DD`; the device speaks `Date`. */

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * `YYYY-MM-DD` as a local `Date` at midnight.
 *
 * The one place that validates the format, so the rest of this module can index
 * the parts without a defensive fallback at every call site.
 */
function parseIso(iso: string): Date {
	const match = ISO_DATE.exec(iso);
	if (match === null) {
		throw new RangeError(`not a YYYY-MM-DD date: ${iso}`);
	}
	return new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
}

/** A `Date` as `YYYY-MM-DD` in the device's own timezone, not UTC. */
export function isoDate(date: Date): string {
	const year = date.getFullYear();
	const month = `${date.getMonth() + 1}`.padStart(2, '0');
	const day = `${date.getDate()}`.padStart(2, '0');
	return `${year}-${month}-${day}`;
}

export function today(): string {
	return isoDate(new Date());
}

/** `YYYY-MM-DD` shifted by whole days, staying in local time. */
export function shiftDays(iso: string, days: number): string {
	const date = parseIso(iso);
	date.setDate(date.getDate() + days);
	return isoDate(date);
}

/** `HH:MM:SS` or `HH:MM` trimmed to `HH:MM` for display. */
export function clockTime(time: string): string {
	return time.slice(0, 5);
}

/** A friendly label: "Today", "Yesterday", "Tomorrow", else a short date. */
export function dayLabel(iso: string, reference: string = today()): string {
	if (iso === reference) return 'Today';
	if (iso === shiftDays(reference, -1)) return 'Yesterday';
	if (iso === shiftDays(reference, 1)) return 'Tomorrow';

	return parseIso(iso).toLocaleDateString(undefined, {
		weekday: 'short',
		day: 'numeric',
		month: 'short'
	});
}

/** Monday of the week containing `iso`. */
export function startOfWeek(iso: string): string {
	// getDay() is Sunday-based; the schedule grammar and the UI are Monday-based.
	const offset = (parseIso(iso).getDay() + 6) % 7;
	return shiftDays(iso, -offset);
}

/** First day of the month containing `iso`. */
export function startOfMonth(iso: string): string {
	return `${iso.slice(0, 7)}-01`;
}

/** Last day of the month containing `iso`. */
export function endOfMonth(iso: string): string {
	const date = parseIso(iso);
	// Day 0 of the next month is the last day of this one.
	return isoDate(new Date(date.getFullYear(), date.getMonth() + 1, 0));
}

/** `YYYY-MM-DD` moved by whole months, landing on the first of the month. */
export function shiftMonths(iso: string, months: number): string {
	const date = parseIso(iso);
	return isoDate(new Date(date.getFullYear(), date.getMonth() + months, 1));
}
