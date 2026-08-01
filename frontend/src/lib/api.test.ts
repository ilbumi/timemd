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
