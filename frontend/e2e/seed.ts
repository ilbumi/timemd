/**
 * Writes the markdown tree the layout suite runs against.
 *
 * The suite needs screens that are *full* — a ragged edge or a doubled rule
 * only draws where there is content to draw it against — so this seeds every
 * list the UI can render: active and archived projects, milestones in both
 * states, a week of sessions, and recurring blocks on every weekday.
 *
 * It writes its own fixture rather than copying `./data`, which is the
 * developer's real tree: the suite has to be reproducible, and it must never
 * be able to mutate someone's time log.
 *
 * Dates are computed from today rather than pinned, because a fixture pinned
 * to a date stops populating the day, week and log screens tomorrow.
 */
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

/** Local calendar date as `YYYY-MM-DD`; `toISOString` would shift the day. */
function isoDay(date: Date): string {
	const month = String(date.getMonth() + 1).padStart(2, '0');
	const day = String(date.getDate()).padStart(2, '0');
	return `${date.getFullYear()}-${month}-${day}`;
}

function daysAgo(count: number): Date {
	const date = new Date();
	date.setDate(date.getDate() - count);
	return date;
}

function write(root: string, relative: string, body: string): void {
	const path = join(root, relative);
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, body.trimStart(), 'utf8');
}

/** A day's sessions. Deliberately uneven so the log's durations vary in width. */
const SESSIONS: Record<number, string[]> = {
	0: [
		'- 09:00-09:25 (25m) [[thesis]] chapter four, first pass',
		'- 09:30-09:55 (25m) [[thesis]]',
		'- 11:00-11:45 (45m) [[atlas]] rewrote the ingest step so it stops on the first bad row',
		'- 14:00-14:25 (25m) [[atlas]]'
	],
	1: [
		'- 08:40-09:05 (25m) [[thesis]] lit review',
		'- 10:00-11:30 (1h30m) [[atlas]] pairing',
		'- 16:00-16:25 (25m) reading'
	],
	2: ['- 09:00-10:15 (1h15m) [[thesis]]', '- 13:00-13:25 (25m) [[atlas]] triage'],
	3: [
		'- 09:05-09:30 (25m) [[thesis]] outline',
		'- 09:35-10:00 (25m) [[thesis]]',
		'- 15:00-15:50 (50m) [[atlas]]'
	],
	4: ['- 10:00-10:25 (25m) [[atlas]]'],
	5: ['- 09:00-09:25 (25m) [[thesis]] notes', '- 09:30-10:20 (50m) [[thesis]]'],
	6: ['- 11:00-11:25 (25m) [[atlas]] weekly review']
};

export function seed(root: string): void {
	rmSync(root, { recursive: true, force: true });

	// UTC keeps "today" the same for the server and this script wherever the
	// suite runs; a fixture that disagrees with the server about the date
	// renders an empty day screen.
	write(
		root,
		'settings.md',
		`
---
timezone: UTC
focus: 25m
short_break: 5m
long_break: 15m
long_break_every: 4
remind_before: 5m
---

# Settings
`
	);

	write(
		root,
		'projects/thesis.md',
		`
---
name: Thesis
color: '#245a8d'
mark: square
target: 12h
status: active
created: ${isoDay(daysAgo(30))}
---

# Thesis

## Milestones

- [x] Ch. 1 — lit review
- [x] Ch. 2 — method
- [ ] Ch. 4 — first draft
- [ ] Submit to committee before the end of the month
`
	);

	write(
		root,
		'projects/atlas.md',
		`
---
name: Atlas ingest
color: '#4a6b63'
mark: bar
target: 6h30m
status: active
created: ${isoDay(daysAgo(20))}
---

# Atlas ingest

## Milestones

- [ ] Ship the retry path
`
	);

	// Enough active projects that the tile shelf has both a full row of four and
	// a short row after it, which is where a reserved empty cell used to show.
	for (const [slug, name, mark, color] of [
		['notes', 'Notes', 'circle', '#8a5a2b'],
		['admin', 'Admin', 'triangle', '#6b4a8a'],
		['reading', 'Reading', 'diamond', '#2b6b7a']
	]) {
		write(
			root,
			`projects/${slug}.md`,
			`
---
name: ${name}
color: '${color}'
mark: ${mark}
status: active
created: ${isoDay(daysAgo(15))}
---

# ${name}
`
		);
	}

	write(
		root,
		'projects/masters-course.md',
		`
---
name: Masters course
mark: diamond
status: archived
created: ${isoDay(daysAgo(90))}
---

# Masters course
`
	);

	// Every weekday carries a block so the week raster is full at all widths,
	// and the day view has both a morning and an afternoon block to place.
	write(
		root,
		'schedule/recurring.md',
		`
---
---

# Recurring schedule

## Blocks

- \`deep-work\` mon-fri 09:00-11:00 [[thesis]] Deep work !5m
- \`ingest\` mon,wed,fri 14:00-15:30 [[atlas]] Atlas ingest !10m
- \`review\` wed 16:00-16:30 Weekly review !5m
- \`reading\` sat,sun 10:00-11:00 [[thesis]] Reading
`
	);

	write(
		root,
		'state/active.md',
		`
---
---

# Active session

Nothing running.
`
	);

	for (const [offset, lines] of Object.entries(SESSIONS)) {
		const date = daysAgo(Number(offset));
		const day = isoDay(date);
		write(
			root,
			`days/${date.getFullYear()}/${day}.md`,
			`
---
date: ${day}
---

# ${day}

## Sessions

${lines.join('\n')}
`
		);
	}
}
