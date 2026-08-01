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
