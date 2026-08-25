import { describe, expect, it } from 'vitest';
import type { Todo } from './api';
import { bandOf, group, stampDate, stampTime, subtitle } from './todos';

const TODAY = '2026-08-10';

function todo(overrides: Partial<Todo> = {}): Todo {
	return {
		id: 'abc123',
		status: 'open',
		description: 'Ship it',
		project: null,
		priority: 'normal',
		tags: [],
		recurrence: null,
		dependsOn: [],
		created: null,
		start: null,
		scheduled: null,
		due: null,
		cancelled: null,
		done: null,
		onCompletion: null,
		...overrides
	};
}

describe('stamps', () => {
	it('splits a stamp into its date and time halves', () => {
		expect(stampDate('2026-08-30 14:00')).toBe('2026-08-30');
		expect(stampTime('2026-08-30 14:00')).toBe('14:00');
	});

	it('reports no time for a stamp that names only a day', () => {
		expect(stampDate('2026-08-30')).toBe('2026-08-30');
		expect(stampTime('2026-08-30')).toBeNull();
		expect(stampDate(null)).toBeNull();
		expect(stampTime(null)).toBeNull();
	});
});

describe('bandOf', () => {
	it('separates yesterday, today, this week and later', () => {
		expect(bandOf(todo({ due: '2026-08-09' }), TODAY)).toBe('overdue');
		expect(bandOf(todo({ due: TODAY }), TODAY)).toBe('today');
		expect(bandOf(todo({ due: '2026-08-16' }), TODAY)).toBe('week');
		expect(bandOf(todo({ due: '2026-08-17' }), TODAY)).toBe('later');
	});

	it('puts a todo with no date last rather than dropping it', () => {
		expect(bandOf(todo(), TODAY)).toBe('someday');
	});

	/* A todo you meant to do on Tuesday belongs on Tuesday's screen whether or
	   not anyone is waiting for it, so the earlier of the two dates decides. */
	it('takes the earlier of the due and scheduled dates', () => {
		expect(bandOf(todo({ due: '2026-09-01', scheduled: TODAY }), TODAY)).toBe('today');
		expect(bandOf(todo({ scheduled: '2026-09-01' }), TODAY)).toBe('later');
	});
});

describe('group', () => {
	it('orders by date, then urgency, then alphabetically', () => {
		const groups = group(
			[
				todo({ id: 'd', description: 'No date' }),
				todo({ id: 'c', description: 'Later', due: '2026-08-17' }),
				todo({ id: 'b', description: 'Today normal', due: TODAY }),
				todo({ id: 'a', description: 'Today urgent', due: TODAY, priority: 'highest' })
			],
			TODAY
		);

		expect(groups.map((entry) => entry.band)).toEqual(['today', 'later', 'someday']);
		expect(groups[0]?.todos.map((entry) => entry.description)).toEqual([
			'Today urgent',
			'Today normal'
		]);
	});

	it('drops a band nothing landed in', () => {
		expect(group([todo({ due: TODAY })], TODAY).map((entry) => entry.label)).toEqual(['Today']);
		expect(group([], TODAY)).toEqual([]);
	});
});

describe('subtitle', () => {
	it('names the project, the dates and whether it repeats', () => {
		expect(
			subtitle(
				todo({
					project: 'timemd',
					due: '2026-08-31',
					scheduled: '2026-08-30 14:00',
					recurrence: 'every day'
				})
			)
		).toBe('timemd · due 2026-08-31 · at 2026-08-30 14:00 · repeats');
	});

	it('is empty when there is nothing to say', () => {
		expect(subtitle(todo())).toBe('');
	});
});
