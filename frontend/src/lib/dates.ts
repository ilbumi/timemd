/** Local-date helpers. The API speaks `YYYY-MM-DD`; the device speaks `Date`. */

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
	const [year, month, day] = iso.split('-').map(Number);
	const shifted = new Date(year ?? 1970, (month ?? 1) - 1, (day ?? 1) + days);
	return isoDate(shifted);
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

	const [year, month, day] = iso.split('-').map(Number);
	return new Date(year ?? 1970, (month ?? 1) - 1, day ?? 1).toLocaleDateString(undefined, {
		weekday: 'short',
		day: 'numeric',
		month: 'short'
	});
}
