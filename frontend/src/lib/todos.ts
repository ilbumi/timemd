/**
 * Ordering and grouping for the todo list.
 *
 * Here rather than in the screen so it can be tested without a DOM: "is this
 * overdue" is a date comparison with an off-by-one in it, and that belongs
 * somewhere a test can reach.
 */

import { shiftDays, today } from '$lib/dates';
import type { Priority, Todo } from '$lib/api';

/** The buckets, in the order they are shown. */
export const BANDS = ['overdue', 'today', 'week', 'later', 'someday'] as const;

export type Band = (typeof BANDS)[number];

export const BAND_LABELS: Record<Band, string> = {
	overdue: 'Overdue',
	today: 'Today',
	week: 'This week',
	later: 'Later',
	someday: 'No date'
};

/** Most urgent first, which is the order the list is sorted in. */
const PRIORITY_ORDER: Priority[] = ['highest', 'high', 'medium', 'normal', 'low', 'lowest'];

/**
 * The date half of a stamp.
 *
 * A stamp is `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`, so the first ten characters
 * are the date and string comparison over them is date comparison — no `Date`,
 * and so no timezone to get wrong.
 */
export function stampDate(stamp: string | null): string | null {
	return stamp === null ? null : stamp.slice(0, 10);
}

/** The time half, or null for a stamp that names only a day. */
export function stampTime(stamp: string | null): string | null {
	if (stamp === null) return null;
	const time = stamp.slice(11);
	return time === '' ? null : time;
}

/**
 * Which band a todo falls in, by the earlier of its due and scheduled dates.
 *
 * Scheduled counts as well as due because a todo you meant to do on Tuesday
 * belongs on Tuesday's screen whether or not anyone is waiting for it.
 */
export function bandOf(todo: Todo, reference: string = today()): Band {
	const date = soonest(todo);
	if (date === null) return 'someday';
	if (date < reference) return 'overdue';
	if (date === reference) return 'today';
	return date <= shiftDays(reference, 6) ? 'week' : 'later';
}

/**
 * The earlier of a todo's due and scheduled dates, or null for a todo with
 * neither. One function because the sort and the banding have to agree on it.
 */
function soonest(todo: Todo): string | null {
	const dates = [stampDate(todo.due), stampDate(todo.scheduled)].filter(
		(date): date is string => date !== null
	);
	return dates.length === 0 ? null : dates.reduce((a, b) => (b < a ? b : a));
}

/** Sort key: soonest first, then most urgent, then alphabetical. */
function rank(todo: Todo): [string, number, string] {
	const priority = PRIORITY_ORDER.indexOf(todo.priority);
	return [
		soonest(todo) ?? '9999-99-99',
		priority === -1 ? PRIORITY_ORDER.length : priority,
		todo.description
	];
}

export interface Group {
	band: Band;
	label: string;
	todos: Todo[];
}

/** Groups todos into the bands, dropping any band nothing landed in. */
export function group(todos: Todo[], reference: string = today()): Group[] {
	// Ranked once per todo rather than once per comparison, and banded in the
	// same pass — both are date arithmetic, and a long list pays for each.
	const ranked = todos.map((todo) => ({ todo, key: rank(todo), band: bandOf(todo, reference) }));
	ranked.sort(
		(left, right) =>
			// Dates compare as strings, not by locale: a collation that ignores
			// the hyphens would order `2026-1` against `2026-11` by accident.
			(left.key[0] < right.key[0] ? -1 : left.key[0] > right.key[0] ? 1 : 0) ||
			left.key[1] - right.key[1] ||
			left.key[2].localeCompare(right.key[2])
	);

	const buckets = new Map<Band, Todo[]>(BANDS.map((band) => [band, []]));
	for (const entry of ranked) buckets.get(entry.band)?.push(entry.todo);

	return BANDS.map((band) => ({
		band,
		label: BAND_LABELS[band],
		todos: buckets.get(band) ?? []
	})).filter((entry) => entry.todos.length > 0);
}

/** The short line of context under a todo: project, dates, repeat. */
export function subtitle(todo: Todo): string {
	const parts: string[] = [];
	if (todo.project !== null) parts.push(todo.project);
	if (todo.due !== null) parts.push(`due ${todo.due}`);
	if (todo.scheduled !== null) parts.push(`at ${todo.scheduled}`);
	if (todo.recurrence !== null) parts.push('repeats');
	return parts.join(' · ');
}
