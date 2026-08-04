/**
 * The numbers the screens put on a bar: tracked totals, planned totals, and how
 * far through a target or a milestone list something is.
 *
 * They live together because they are all read off the same two endpoints and
 * all had copies on more than one screen — the report endpoint already groups
 * the per-project totals, and doing the rest here keeps one shape of the answer
 * and one place that knows a null key means "no project".
 */

import { api, type Milestone, type Occurrence, type Project, type Report } from './api';
import { parseMinutes } from './countdown';
import { shiftDays, startOfWeek, today } from './dates';

/** How far back a project's lifetime figure looks. The server refuses a longer
    range, so this is the widest window it will answer. */
const LIFETIME_DAYS = 365;

/** The stepper's ceiling, in minutes. A week has 168 hours; anything near it is
    a typo. Shared with the create screen so a project cannot be given a target
    on one screen that the other can never edit it back to. */
export const MAX_TARGET = 60 * 60;

/** Half-hours, so a `1h30m` target written by hand survives an edit instead of
    being rounded to the nearest hour. */
export const TARGET_STEP = 30;

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

/**
 * A project's weekly target in minutes, or zero when it has none.
 *
 * `parseMinutes` already reads an absent target as zero, so there is no null
 * case to guard — which is why this is one line and lives in one place.
 */
export function targetMinutes(project: Project): number {
	return parseMinutes(project.target ?? '');
}

/**
 * Minutes a set of schedule blocks sets aside.
 *
 * The server's `duration` is used rather than end-minus-start because it is the
 * one that survives a block crossing midnight — and having every schedule screen
 * agree on that is the point of it living here.
 */
export function plannedMinutes(blocks: Occurrence[]): number {
	return blocks.reduce((total, block) => total + parseMinutes(block.duration), 0);
}

/** How many of a project's milestones are done. */
export function doneCount(milestones: Milestone[]): number {
	return milestones.reduce((done, milestone) => done + (milestone.done ? 1 : 0), 0);
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
