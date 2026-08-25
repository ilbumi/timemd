import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, api } from './api';

interface FetchCall {
	url: string;
	init: RequestInit;
}

function mockFetch(status: number, body: unknown): FetchCall[] {
	const calls: FetchCall[] = [];
	vi.stubGlobal('fetch', (url: string, init: RequestInit = {}) => {
		calls.push({ url, init });
		return Promise.resolve(
			new Response(status === 204 ? null : JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
	});
	return calls;
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('api', () => {
	it('lists projects', async () => {
		const calls = mockFetch(200, [{ slug: 'timemd', name: 'timemd' }]);
		const projects = await api.listProjects();

		expect(projects).toHaveLength(1);
		expect(projects[0]?.slug).toBe('timemd');
		expect(calls[0]?.url).toBe('/api/projects');
		expect(calls[0]?.init.method).toBe('GET');
	});

	it('sends a JSON body when creating', async () => {
		const calls = mockFetch(201, { slug: 'deep-work', name: 'Deep Work' });
		await api.createProject({ name: 'Deep Work' });

		expect(calls[0]?.init.method).toBe('POST');
		expect(calls[0]?.init.body).toBe('{"name":"Deep Work"}');
		expect(calls[0]?.init.headers).toEqual({ 'content-type': 'application/json' });
	});

	it('escapes slugs into the path', async () => {
		const calls = mockFetch(200, { slug: 'a b' });
		await api.readProject('a b');

		expect(calls[0]?.url).toBe('/api/projects/a%20b');
	});

	it('resolves without a body on 204', async () => {
		mockFetch(204, null);
		await expect(api.deleteProject('timemd')).resolves.toBeUndefined();
	});

	it('surfaces the server message on failure', async () => {
		mockFetch(409, { error: 'a project named "timemd" already exists' });

		await expect(api.createProject({ name: 'timemd' })).rejects.toMatchObject({
			status: 409,
			message: 'a project named "timemd" already exists'
		});
	});

	it('falls back to a generic message when the body carries none', async () => {
		mockFetch(500, {});

		await expect(api.listProjects()).rejects.toMatchObject({
			status: 500,
			message: 'Request failed with 500'
		});
	});

	it('reports an unreachable server distinctly', async () => {
		vi.stubGlobal('fetch', () => Promise.reject(new TypeError('Failed to fetch')));

		const failure = await api.listProjects().catch((error: unknown) => error);
		expect(failure).toBeInstanceOf(ApiError);
		expect(failure).toMatchObject({ status: 0, message: 'Could not reach the server' });
	});

	it('sends a patch as JSON', async () => {
		const calls = mockFetch(200, { slug: 'timemd', status: 'archived' });
		await api.updateProject('timemd', { status: 'archived' });

		expect(calls[0]?.init.method).toBe('PATCH');
		expect(calls[0]?.init.body).toBe('{"status":"archived"}');
	});

	it('sends the milestone list whole, because the server replaces it whole', async () => {
		const calls = mockFetch(200, { slug: 'thesis', milestones: [] });
		await api.updateProject('thesis', { milestones: [{ done: true, title: 'Ch. 1' }] });

		expect(calls[0]?.init.body).toBe('{"milestones":[{"done":true,"title":"Ch. 1"}]}');
	});

	it('creates with a mark and a target', async () => {
		const calls = mockFetch(201, { slug: 'thesis' });
		await api.createProject({ name: 'Thesis', mark: 'triangle', target: '10h' });

		expect(calls[0]?.init.body).toBe('{"name":"Thesis","mark":"triangle","target":"10h"}');
	});
});

describe('settings', () => {
	it('reads the durations', async () => {
		const calls = mockFetch(200, { focus: '25m', shortBreak: '5m' });
		const settings = await api.readSettings();

		expect(settings.focus).toBe('25m');
		expect(calls[0]?.url).toBe('/api/settings');
	});

	it('writes only the keys it is given', async () => {
		const calls = mockFetch(200, { focus: '50m' });
		await api.writeSettings({ focus: '50m' });

		expect(calls[0]?.init.method).toBe('PUT');
		expect(calls[0]?.init.body).toBe('{"focus":"50m"}');
	});
});

describe('ntfy', () => {
	it('reads and writes the config', async () => {
		const calls = mockFetch(200, {
			server: 'https://ntfy.sh',
			topic: 'timemd-a7f3',
			appUrl: null,
			hasToken: false,
			subscribeUrl: 'https://ntfy.sh/timemd-a7f3',
			test: 'delivered'
		});

		const config = await api.readNtfy();
		expect(config.subscribeUrl).toBe('https://ntfy.sh/timemd-a7f3');
		expect(calls[0]?.url).toBe('/api/ntfy');

		await api.writeNtfy({ topic: 'timemd-a7f3' });
		expect(calls[1]?.init.method).toBe('PUT');
		expect(calls[1]?.init.body).toBe('{"topic":"timemd-a7f3"}');
	});

	/// A `?? undefined` slip here would make "turn it off" a silent no-op: the
	/// API reads an absent key as "leave it alone".
	it('sends null to clear the topic', async () => {
		const calls = mockFetch(200, { server: 'https://ntfy.sh', topic: null });
		await api.writeNtfy({ topic: null });

		expect(calls[0]?.init.body).toBe('{"topic":null}');
	});
});

describe('timer', () => {
	const idle = {
		active: null,
		completedToday: 0,
		trackedToday: '0m',
		nextBreak: '5m',
		nextBreakKind: 'short_break',
		serverNow: '2026-08-01T09:00:00'
	};

	it('reads the current state', async () => {
		const calls = mockFetch(200, idle);
		const state = await api.readTimer();

		expect(state.active).toBeNull();
		expect(state.nextBreak).toBe('5m');
		expect(calls[0]?.url).toBe('/api/timer');
	});

	it('starts a session with the given shape', async () => {
		const calls = mockFetch(200, idle);
		await api.startSession({ kind: 'focus', project: 'timemd', note: 'file store' });

		expect(calls[0]?.url).toBe('/api/timer/start');
		expect(calls[0]?.init.body).toBe('{"kind":"focus","project":"timemd","note":"file store"}');
	});

	it('stops and cancels with an empty body', async () => {
		const calls = mockFetch(200, idle);
		await api.stopSession();
		await api.cancelSession();

		expect(calls.map((call) => call.url)).toEqual(['/api/timer/stop', '/api/timer/cancel']);
		expect(calls.every((call) => call.init.body === '{}')).toBe(true);
	});
});

describe('days and schedule', () => {
	const day = {
		date: '2026-08-05',
		sessions: [],
		tracked: '0m',
		planned: [],
		skipped: [],
		problems: []
	};

	it('reads a day by date', async () => {
		const calls = mockFetch(200, day);
		const result = await api.readDay('2026-08-05');

		expect(result.date).toBe('2026-08-05');
		expect(calls[0]?.url).toBe('/api/days/2026-08-05');
	});

	it('adds, edits and deletes sessions', async () => {
		const calls = mockFetch(204, null);
		await api.addSession('2026-08-05', { start: '09:00:00', end: '10:00:00', note: 'work' });
		await api.updateSession('2026-08-05', 1, { start: '09:00:00', end: '09:30:00' });
		await api.deleteSession('2026-08-05', 1);

		expect(calls.map((call) => `${call.init.method} ${call.url}`)).toEqual([
			'POST /api/days/2026-08-05/sessions',
			'PATCH /api/days/2026-08-05/sessions/1',
			'DELETE /api/days/2026-08-05/sessions/1'
		]);
	});

	it('adds, edits and deletes one-off blocks', async () => {
		const calls = mockFetch(204, null);
		await api.addBlock('2026-08-05', { start: '12:00:00', end: '12:30:00', title: 'Lunch' });
		await api.updateBlock('2026-08-05', 0, {
			start: '12:15:00',
			end: '13:00:00',
			title: 'Long lunch'
		});
		await api.deleteBlock('2026-08-05', 0);

		expect(calls.map((call) => `${call.init.method} ${call.url}`)).toEqual([
			'POST /api/days/2026-08-05/blocks',
			'PATCH /api/days/2026-08-05/blocks/0',
			'DELETE /api/days/2026-08-05/blocks/0'
		]);
		expect(calls[0]?.init.body).toContain('"title":"Lunch"');
		expect(calls[1]?.init.body).toContain('"title":"Long lunch"');
	});

	it('skips and restores a repeating block', async () => {
		const calls = mockFetch(204, null);
		await api.skipBlock('2026-08-05', 'deep-work');
		await api.unskipBlock('2026-08-05', 'deep-work');

		expect(calls[0]?.init.body).toBe('{"id":"deep-work"}');
		expect(calls[1]?.url).toBe('/api/days/2026-08-05/skips/deep-work');
	});

	it('reads an expanded range', async () => {
		const calls = mockFetch(200, []);
		await api.readSchedule('2026-08-01', '2026-08-07');

		expect(calls[0]?.url).toBe('/api/schedule?from=2026-08-01&to=2026-08-07');
	});

	it('reads and replaces the repeating list', async () => {
		const block = {
			id: 'deep-work',
			days: ['mon', 'tue', 'wed', 'thu', 'fri'],
			start: '09:00:00',
			end: '11:00:00',
			project: null,
			title: 'Deep work',
			remindBefore: '5m'
		};
		const calls = mockFetch(200, [block]);

		await api.readRecurring();
		await api.writeRecurring([block]);

		expect(calls[0]?.init.method).toBe('GET');
		expect(calls[1]?.init.method).toBe('PUT');
		expect(calls[1]?.init.body).toContain('"id":"deep-work"');
	});
});

describe('reports', () => {
	it('passes the range and grouping through', async () => {
		const calls = mockFetch(200, {
			from: '2026-08-01',
			to: '2026-08-31',
			groupBy: 'day',
			total: '3h',
			planned: '4h',
			buckets: []
		});
		const report = await api.readReport('2026-08-01', '2026-08-31', 'day');

		expect(report.total).toBe('3h');
		expect(report.planned).toBe('4h');
		expect(calls[0]?.url).toBe('/api/reports?from=2026-08-01&to=2026-08-31&groupBy=day');
	});
});

describe('todos', () => {
	it('sends only the filters that were given', async () => {
		const calls = mockFetch(200, { todos: [], problems: [] });
		await api.listTodos({ status: 'open', dueBefore: '2026-08-31' });

		expect(calls[0]?.url).toBe('/api/todos?status=open&dueBefore=2026-08-31');
	});

	it('asks for everything when no filter is given', async () => {
		const calls = mockFetch(200, { todos: [], problems: [] });
		await api.listTodos();

		expect(calls[0]?.url).toBe('/api/todos');
	});

	/* An absent key means "leave it"; an explicit null means "clear it", and
	   the client has to be able to spell the second one. */
	it('sends an explicit null to clear a date', async () => {
		const calls = mockFetch(200, { id: 'abc123' });
		await api.updateTodo('abc123', { due: null });

		expect(calls[0]?.init.method).toBe('PATCH');
		expect(calls[0]?.init.body).toBe('{"due":null}');
		expect(calls[0]?.url).toBe('/api/todos/abc123');
	});

	it('escapes an id into the path', async () => {
		mockFetch(204, null);
		const calls = mockFetch(204, null);
		await api.deleteTodo('a b');

		expect(calls[0]?.url).toBe('/api/todos/a%20b');
	});
});
