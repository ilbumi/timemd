/**
 * The pomodoro cycle across a break — the one flow that spans four of the
 * timer's five screens and cannot be seen from any single one of them.
 *
 * Its own file rather than a case in `screens.test.ts`: this drives a *sequence*
 * of timer reads, so the stub has to be a queue rather than a constant, and
 * making `screens.test.ts` serve one would change what every other screen sees.
 * `make e2e` cannot cover it either — it stubs `GET /api/timer` to hold one
 * state still, which is the opposite of what is under test here.
 */

import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { SessionKind, StartSession, TimerState } from '$lib/api';
import { fullscreen } from '$lib/fullscreen.svelte';

import Timer from './+page.svelte';

vi.mock('$app/state', () => ({
	get page() {
		return { url: new URL('http://localhost/'), params: {} };
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
	milestones: [],
	problems: []
};

const NOTE = 'chapter four';

/** The block the day file holds once the focus session has been logged. */
const LOGGED = {
	index: 0,
	start: '09:00:00',
	end: '09:25:00',
	duration: '25m',
	project: 'thesis',
	note: NOTE
};

const DAY = {
	date: '2026-08-01',
	tracked: '25m',
	sessions: [LOGGED],
	planned: [],
	skipped: [],
	problems: []
};

const REPORT = {
	from: '2026-07-27',
	to: '2026-08-02',
	groupBy: 'project',
	total: '25m',
	planned: '0m',
	buckets: [{ key: 'thesis', tracked: '25m', planned: '0m', sessions: 1 }]
};

const SETTINGS = {
	timezone: 'UTC',
	focus: '25m',
	shortBreak: '5m',
	longBreak: '15m',
	longBreakEvery: 4,
	remindBefore: '5m'
};

/** Sessions on the board once the focus block has been logged. */
const COMPLETED = 3;

/**
 * Long enough for several of the component's 250ms ticks. The queue advances on
 * a poll, not on a clock, so waiting is the only way to reach the next state.
 */
const SETTLING = { timeout: 3000 };

function idle(completedToday: number): TimerState {
	return {
		active: null,
		completedToday,
		trackedToday: '25m',
		nextBreak: '5m',
		nextBreakKind: 'short_break',
		serverNow: '2026-08-01T09:25:00'
	};
}

/**
 * A block whose deadline has already passed, so the 250ms tick re-polls at once
 * and the queue advances without waiting out a real countdown.
 */
function running(
	kind: SessionKind,
	project: string | null,
	note: string,
	completedToday: number,
	remainingSeconds = 0
): TimerState {
	return {
		...idle(completedToday),
		active: {
			kind,
			project,
			note,
			startedAt: '2026-08-01T09:00:00',
			endsAt: '2026-08-01T09:25:00',
			duration: '25m',
			durationSeconds: 1500,
			remainingSeconds
		}
	};
}

/**
 * What the real server answers a start with: the block it was actually asked
 * for. Echoing the request rather than serving a canned block is what makes
 * these tests able to fail — a stub that hands back a project the client never
 * sent would hide the very bug under test.
 */
function started(request: StartSession): TimerState {
	return running(request.kind ?? 'focus', request.project ?? null, request.note ?? '', COMPLETED);
}

/** Bodies the component sent to `/api/timer/start`, in order. */
let startBodies: StartSession[] = [];
let states: TimerState[] = [];
let dayStatus = 200;

/** The head of the queue, advancing until only the resting state is left. */
function nextState(): TimerState {
	const head = states.length > 1 ? states.shift() : states[0];
	if (head === undefined) {
		// An empty queue means the test forgot to set one up; saying so beats
		// answering `undefined` and failing somewhere in the component.
		throw new Error('the timer stub has no states left to serve');
	}
	return head;
}

function stubApi(): void {
	vi.stubGlobal('fetch', (url: string, init?: RequestInit) => {
		const method = init?.method ?? 'GET';
		const body: StartSession = init?.body === undefined ? {} : JSON.parse(String(init.body));

		if (url.startsWith('/api/days') && method === 'PATCH') {
			return Promise.resolve(
				new Response(JSON.stringify({ error: 'the day file is read-only' }), {
					status: dayStatus,
					headers: { 'content-type': 'application/json' }
				})
			);
		}

		const payload = (() => {
			if (url.startsWith('/api/projects')) return [PROJECT];
			if (url.startsWith('/api/days')) return DAY;
			if (url.startsWith('/api/reports')) return REPORT;
			if (url.startsWith('/api/settings')) return SETTINGS;
			if (url === '/api/timer/start') {
				startBodies.push(body);
				return started(body);
			}
			if (url.startsWith('/api/timer')) return nextState();
			return {};
		})();

		return Promise.resolve(
			new Response(JSON.stringify(payload), { headers: { 'content-type': 'application/json' } })
		);
	});
}

/**
 * Renders mid-focus, lets the block finish, and lands on the completion screen.
 * Owns the queue that gets there, since all three cases start the same way.
 */
async function reachCompletionScreen(): Promise<HTMLElement> {
	states = [running('focus', PROJECT.slug, NOTE, COMPLETED - 1), idle(COMPLETED)];
	stubApi();
	render(Timer);
	return await vi.waitFor(() => screen.getByRole('button', { name: /take break/i }), SETTLING);
}

beforeEach(() => {
	startBodies = [];
	states = [];
	dayStatus = 200;
});

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('continuing the cycle across a break', () => {
	/**
	 * The break is where the selection used to die: it was started with no
	 * project and no note, and reading that back cleared both.
	 */
	it('starts the break against the block it is a break from', async () => {
		(await reachCompletionScreen()).click();

		await vi.waitFor(() => {
			expect(startBodies).toHaveLength(1);
		});
		expect(startBodies[0]).toEqual({
			kind: 'short_break',
			project: PROJECT.slug,
			note: NOTE
		});
	});

	/** The reported bug: every break cost a full re-selection to carry on. */
	it('offers the same block again once the break ends', async () => {
		(await reachCompletionScreen()).click();

		// The break's own reply is built from what the client sent, so a wiped
		// selection here would be the client's doing, not the stub's.
		const tile = await vi.waitFor(
			() => screen.getByRole('button', { name: new RegExp(PROJECT.name, 'i') }),
			SETTLING
		);
		expect(tile).toHaveAttribute('aria-pressed', 'true');
		expect(screen.getByLabelText(/note/i)).toHaveValue(NOTE);
	});

	/**
	 * The note edit and the message explaining why it failed both used to be
	 * swallowed: `run` clears the error on its way into the break.
	 */
	it('does not start the break when the note could not be saved', async () => {
		dayStatus = 500;

		const takeBreak = await reachCompletionScreen();
		const written = screen.getByLabelText(/what got done/i);
		await fireEvent.input(written, { target: { value: 'chapter five' } });
		takeBreak.click();

		await vi.waitFor(() => {
			expect(screen.getByRole('alert')).toHaveTextContent('the day file is read-only');
		});
		expect(startBodies).toHaveLength(0);
	});
});

describe('the idle screen', () => {
	it('offers the next block once one is done', async () => {
		states = [idle(2)];
		stubApi();
		render(Timer);

		await vi.waitFor(() => {
			expect(screen.getByRole('button', { name: /start next/i })).toBeInTheDocument();
		});
		expect(screen.getByText(/focus 03/i)).toBeInTheDocument();
	});

	it('reads as a fresh start before anything is done', async () => {
		states = [idle(0)];
		stubApi();
		render(Timer);

		await vi.waitFor(() => {
			expect(screen.getByRole('button', { name: /^start$/i })).toBeInTheDocument();
		});
		expect(screen.queryByText(/focus 0/i)).not.toBeInTheDocument();
	});
});

describe('stopping under a minute', () => {
	it('says nothing was logged rather than returning to idle as if it was', async () => {
		states = [
			{
				...idle(0),
				trackedToday: '0m',
				active: {
					kind: 'focus',
					project: PROJECT.slug,
					note: NOTE,
					startedAt: '2026-08-01T09:00:00',
					endsAt: '2026-08-01T09:25:00',
					duration: '25m',
					durationSeconds: 1500,
					remainingSeconds: 1498
				}
			},
			{ ...idle(0), trackedToday: '0m', stopped: 'tooShort' }
		];
		stubApi();
		render(Timer);

		(await vi.waitFor(() => screen.getByRole('button', { name: /stop and log/i }))).click();

		await vi.waitFor(() => {
			expect(screen.getByRole('alert')).toHaveTextContent(/under a minute/i);
		});
		expect(screen.getByText(/ready/i)).toBeInTheDocument();
		expect(screen.queryByText(/session complete/i)).not.toBeInTheDocument();

		// A visibility refresh used to go through `run`, which clears `error`,
		// so the banner vanished the moment the tab was looked at again.
		document.dispatchEvent(new Event('visibilitychange'));
		await vi.waitFor(() => {
			expect(screen.getByRole('alert')).toHaveTextContent(/under a minute/i);
		});
	});
});

/**
 * Fullscreen is a mode of the *session*, which is the half that cannot be seen
 * from the component alone: the button turns it on, and the block ending has to
 * turn it back off — there is no fullscreen control on the screens that follow.
 */
describe('fullscreen mode', () => {
	/** A block with time left on it, so the poll queue does not advance under us. */
	const holding = (): TimerState => running('focus', PROJECT.slug, NOTE, COMPLETED, 900);

	afterEach(() => {
		fullscreen.active = false;
	});

	it('fills the screen, and gives it back when the block is logged', async () => {
		states = [holding(), idle(COMPLETED + 1)];
		stubApi();
		render(Timer);

		const fill = await vi.waitFor(
			() => screen.getByRole('button', { name: /^fullscreen$/i }),
			SETTLING
		);
		await fireEvent.click(fill);
		expect(fullscreen.active).toBe(true);
		expect(screen.getByRole('button', { name: /leave fullscreen/i })).toBeInTheDocument();

		await fireEvent.click(screen.getByRole('button', { name: /stop and log/i }));
		await vi.waitFor(() => {
			expect(fullscreen.active).toBe(false);
		}, SETTLING);
	});
});
