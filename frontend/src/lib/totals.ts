/**
 * Per-project totals over a date range.
 *
 * The projects screens all want the same two numbers — how long and how many
 * sessions — for a whole set of projects at once, which is exactly what the
 * report endpoint already groups. Doing it here rather than per screen keeps
 * one shape of the answer and one place that knows a null key means "no
 * project".
 */

import { api, type Report } from './api';
import { parseMinutes } from './countdown';

export interface Totals {
	/** Whole minutes tracked in the range. */
	tracked: number;
	sessions: number;
}

const NOTHING: Totals = { tracked: 0, sessions: 0 };

/** Rows keyed by project slug. Untagged time has no slug and is left out. */
export function totalsFrom(report: Report): Record<string, Totals> {
	return Object.fromEntries(
		report.buckets
			.filter((bucket) => bucket.key !== null)
			.map((bucket) => [
				bucket.key,
				{ tracked: parseMinutes(bucket.tracked), sessions: bucket.sessions }
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
