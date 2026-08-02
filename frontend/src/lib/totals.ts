/**
 * Per-project totals over a date range.
 *
 * The projects screens all want the same two numbers — how long and how many
 * sessions — for a whole set of projects at once, which is exactly what the
 * report endpoint already groups. Doing it here rather than per screen keeps
 * one shape of the answer and one place that knows a null key means "no
 * project".
 */

import { api, type Project, type Report } from './api';
import { parseMinutes } from './countdown';
import { shiftDays, startOfWeek, today } from './dates';

/** How far back a project's lifetime figure looks. The server refuses a longer
    range, so this is the widest window it will answer. */
const LIFETIME_DAYS = 365;

export interface Totals {
	/** Whole minutes tracked in the range. */
	tracked: number;
	sessions: number;
	/** Whole minutes the schedule set aside over the same range. */
	planned: number;
}

const NOTHING: Totals = { tracked: 0, sessions: 0, planned: 0 };

/** Rows keyed by project slug. Untagged time has no slug and is left out. */
export function totalsFrom(report: Report): Record<string, Totals> {
	return Object.fromEntries(
		report.buckets
			.filter((bucket) => bucket.key !== null)
			.map((bucket) => [
				bucket.key,
				{
					tracked: parseMinutes(bucket.tracked),
					sessions: bucket.sessions,
					planned: parseMinutes(bucket.planned)
				}
			])
	);
}

export function totalsFor(rows: Record<string, Totals>, slug: string): Totals {
	return rows[slug] ?? NOTHING;
}

/** Fetches and shapes in one step. Resolves to nothing rather than throwing:
    every caller draws an empty bar instead of taking the screen down. */
export async function readTotals(from: string, to: string): Promise<Record<string, Totals>> {
	try {
		return totalsFrom(await api.readReport(from, to, 'project'));
	} catch {
		return {};
	}
}

/**
 * A project's weekly target in minutes, or zero when it has none.
 *
 * `parseMinutes` already reads an absent target as zero, so there is no null
 * case to guard — which is why this is one line and lives in one place.
 */
export function targetMinutes(project: Project): number {
	return parseMinutes(project.target ?? '');
}

/** How full the target bar is, clamped so an overshoot does not overflow it. */
export function targetFill(tracked: number, goal: number): number {
	return goal === 0 ? 0 : Math.min(100, (tracked / goal) * 100);
}

/** Totals for the week containing today — what every target bar is measured against. */
export function readWeekTotals(): Promise<Record<string, Totals>> {
	const monday = startOfWeek(today());
	return readTotals(monday, shiftDays(monday, 6));
}

/** Totals over the last year, for the "N logged" on an archived project. */
export function readLifetimeTotals(): Promise<Record<string, Totals>> {
	return readTotals(shiftDays(today(), -LIFETIME_DAYS), today());
}
