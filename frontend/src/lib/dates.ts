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

/** `HH:MM` back to the `HH:MM:SS` the API writes. The inverse of `clockTime`,
    so the wire format is spelled in one module rather than in every form. */
export function withSeconds(time: string): string {
	return `${clockTime(time)}:00`;
}

// Constructing an `Intl.DateTimeFormat` is the expensive part of formatting a
// date, so each is built once — but lazily, because the timer screen imports
// this module for `today()` alone and should not pay for three ICU formatters
// it never uses.
const formatter = (options: Intl.DateTimeFormatOptions) => {
	let built: Intl.DateTimeFormat | undefined;
	return (date: Date) => (built ??= new Intl.DateTimeFormat(undefined, options)).format(date);
};

const shortDate = formatter({ weekday: 'short', day: 'numeric', month: 'short' });
const weekday = formatter({ weekday: 'long' });
const dayAndMonth = formatter({ day: 'numeric', month: 'short' });

/**
 * The three labels `dayLabel` answers without formatting anything.
 *
 * Held from one call to the next because the two list screens ask about a whole
 * week against the same reference, and deriving them is two regex matches and
 * three `Date`s — more work than the comparison they exist to serve.
 */
let relative = { reference: '', yesterday: '', tomorrow: '' };

/** A friendly label: "Today", "Yesterday", "Tomorrow", else a short date. */
export function dayLabel(iso: string, reference: string = today()): string {
	if (iso === reference) return 'Today';
	if (relative.reference !== reference) {
		relative = {
			reference,
			yesterday: shiftDays(reference, -1),
			tomorrow: shiftDays(reference, 1)
		};
	}
	if (iso === relative.yesterday) return 'Yesterday';
	if (iso === relative.tomorrow) return 'Tomorrow';

	return shortDate(parseIso(iso));
}

/** The weekday's own name — "Friday" — for a header that shows the date below it. */
export function weekdayName(iso: string): string {
	return weekday(parseIso(iso));
}

/** Day and month without the weekday — "1 Aug". */
export function monthDay(iso: string): string {
	return dayAndMonth(parseIso(iso));
}

/** Monday of the week containing `iso`. */
export function startOfWeek(iso: string): string {
	// getDay() is Sunday-based; the schedule grammar and the UI are Monday-based.
	const offset = (parseIso(iso).getDay() + 6) % 7;
	return shiftDays(iso, -offset);
}

/**
 * ISO-8601 week number, which is what the week view titles itself with.
 *
 * ISO weeks run Monday to Sunday and week 1 is the one containing the first
 * Thursday — which is why this pivots on Thursday rather than counting days
 * from January 1st.
 */
export function isoWeek(iso: string): number {
	const thursday = parseIso(shiftDays(startOfWeek(iso), 3));
	const firstThursday = parseIso(shiftDays(startOfWeek(`${thursday.getFullYear()}-01-04`), 3));
	return Math.round((thursday.getTime() - firstThursday.getTime()) / 604_800_000) + 1;
}

/** Monday to Sunday of the week containing `iso`. */
export function weekDates(iso: string): string[] {
	const monday = startOfWeek(iso);
	return Array.from({ length: 7 }, (_, offset) => shiftDays(monday, offset));
}

/** Minutes since midnight for an `HH:MM` or `HH:MM:SS` time. */
export function minutesOfDay(time: string): number {
	const [hours, minutes] = clockTime(time).split(':');
	return Number(hours ?? 0) * 60 + Number(minutes ?? 0);
}
