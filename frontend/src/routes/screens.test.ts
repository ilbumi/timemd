/**
 * Every screen renders against a stubbed API without throwing.
 *
 * `svelte-check` proves the markup type-checks; it cannot prove that an effect
 * or a derived does not blow up the first time it runs. This is the cheapest
 * test that would have caught a screen that is blank in a real browser.
 */

import { render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import Layout from './+layout.svelte';
import Timer from './+page.svelte';
import Todos from './todos/+page.svelte';
import ProjectDetail from './projects/[slug]/+page.svelte';
import Projects from './projects/+page.svelte';
import NewProject from './projects/new/+page.svelte';
import Day from './schedule/+page.svelte';
import Log from './schedule/log/+page.svelte';
import Pattern from './schedule/pattern/+page.svelte';
import Week from './schedule/week/+page.svelte';
import Settings from './settings/+page.svelte';

const state = { url: new URL('http://localhost/'), params: { slug: 'thesis' } };

vi.mock('$app/state', () => ({
	get page() {
		return state;
	}
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

const PROJECT = {
	slug: 'thesis',
	name: 'Thesis',
	color: '#245a8d',
	mark: 'triangle',
	target: '10h',
	status: 'active',
	created: '2026-08-01',
	milestones: [
		{ done: true, title: 'Ch. 1' },
		{ done: false, title: 'Ch. 4' }
	],
	problems: []
};

const TODO = {
	id: 'abc123',
	status: 'open',
	description: 'Draft the release notes',
	project: 'thesis',
	priority: 'high',
	tags: [],
	recurrence: null,
	dependsOn: [],
	created: '2026-08-01',
	start: null,
	scheduled: '2026-08-01 14:00',
	due: '2026-08-31',
	cancelled: null,
	done: null,
	onCompletion: null
};

const DAY = {
	date: '2026-08-01',
	tracked: '1h15m',
	sessions: [
		{ index: 0, start: '09:00:00', end: '09:50:00', duration: '50m', project: 'thesis', note: 'A' }
	],
	planned: [
		{
			date: '2026-08-01',
			start: '09:00:00',
			end: '11:00:00',
			duration: '2h',
			project: 'thesis',
			title: 'Deep work',
			remindBefore: '10m',
			block: 'deep-work',
			oneOffIndex: null
		}
	],
	skipped: [],
	todos: [TODO],
	problems: []
};

/** Answers whatever the screen under test asks for, by path. */
function stubApi(): void {
	vi.stubGlobal('fetch', (url: string) => {
		const body = (() => {
			if (url.startsWith('/api/projects/')) return PROJECT;
			if (url.startsWith('/api/projects')) return [PROJECT];
			// Echoes the date it was asked for, as the real endpoint does — the
			// week views key their rows on it.
			if (url.startsWith('/api/days')) {
				return { ...DAY, date: url.slice('/api/days/'.length, '/api/days/'.length + 10) };
			}
			if (url.startsWith('/api/schedule/recurring')) {
				return [
					{
						id: 'deep-work',
						days: ['mon', 'tue', 'wed', 'thu', 'fri'],
						start: '09:00:00',
						end: '11:00:00',
						project: 'thesis',
						title: 'Deep work',
						remindBefore: '10m'
					}
				];
			}
			if (url.startsWith('/api/schedule')) return DAY.planned;
			if (url.startsWith('/api/todos')) return { todos: [TODO], problems: [] };
			if (url.startsWith('/api/reports')) {
				return {
					from: '2026-07-27',
					to: '2026-08-02',
					groupBy: 'project',
					total: '5h',
					planned: '6h',
					buckets: [{ key: 'thesis', tracked: '5h', planned: '6h', sessions: 6 }]
				};
			}
			if (url.startsWith('/api/settings')) {
				return {
					timezone: 'UTC',
					focus: '25m',
					shortBreak: '5m',
					longBreak: '15m',
					longBreakEvery: 4,
					remindBefore: '5m'
				};
			}
			if (url.startsWith('/api/ntfy')) {
				return {
					server: 'https://ntfy.sh',
					topic: null,
					appUrl: null,
					hasToken: false,
					subscribeUrl: null,
					test: null
				};
			}
			if (url.startsWith('/api/timer')) {
				return {
					active: null,
					completedToday: 2,
					trackedToday: '1h15m',
					nextBreak: '5m',
					nextBreakKind: 'short_break',
					serverNow: '2026-08-01T12:00:00'
				};
			}
			return {};
		})();

		return Promise.resolve(
			new Response(JSON.stringify(body), { headers: { 'content-type': 'application/json' } })
		);
	});
}

afterEach(() => {
	vi.unstubAllGlobals();
	state.url = new URL('http://localhost/');
});

const SCREENS = [
	{ name: 'timer', component: Timer, path: '/', heading: /ready/i },
	{ name: 'projects', component: Projects, path: '/projects', heading: /projects/i },
	{ name: 'new project', component: NewProject, path: '/projects/new', heading: /new project/i },
	{
		name: 'project detail',
		component: ProjectDetail,
		path: '/projects/thesis',
		heading: /thesis/i
	},
	{ name: 'todos', component: Todos, path: '/todos', heading: /todos/i },
	{ name: 'day', component: Day, path: '/schedule', heading: /day/i },
	{ name: 'week', component: Week, path: '/schedule/week', heading: /week/i },
	{ name: 'log', component: Log, path: '/schedule/log', heading: /log/i },
	{ name: 'pattern', component: Pattern, path: '/schedule/pattern', heading: /pattern/i },
	{ name: 'settings', component: Settings, path: '/settings', heading: /settings/i }
];

describe('screens', () => {
	for (const { name, component, path, heading } of SCREENS) {
		it(`renders ${name}`, async () => {
			stubApi();
			state.url = new URL(`http://localhost${path}`);

			render(component);

			await vi.waitFor(() => {
				expect(screen.getAllByText(heading).length).toBeGreaterThan(0);
			});
		});
	}

	it('renders the four-mark tab bar', () => {
		stubApi();
		const children = createRawSnippet(() => ({ render: () => '<p>screen</p>' }));
		render(Layout, { children });

		for (const label of ['Timer', 'Projects', 'Todos', 'Schedule']) {
			expect(screen.getByRole('link', { name: label })).toBeInTheDocument();
		}
	});
});
