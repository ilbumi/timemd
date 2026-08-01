/**
 * The JSON client.
 *
 * Every screen goes through here, so the error shape and the request plumbing
 * live in one place rather than being re-derived per component.
 */

/** A JSON document. Modelled properly rather than escaping to `unknown`. */
export type JsonValue =
	string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

export type ProjectStatus = 'active' | 'archived';

export interface Project {
	slug: string;
	name: string;
	color: string | null;
	status: ProjectStatus;
	created: string | null;
}

export interface NewProject {
	name: string;
	color?: string | null;
}

export type SessionKind = 'focus' | 'short_break' | 'long_break';

export interface Running {
	kind: SessionKind;
	project: string | null;
	note: string;
	startedAt: string;
	endsAt: string;
	duration: string;
	/** Seconds left at `serverNow`. */
	remainingSeconds: number;
}

export interface TimerState {
	active: Running | null;
	completedToday: number;
	trackedToday: string;
	nextBreak: string;
	nextBreakKind: SessionKind;
	serverNow: string;
}

export interface StartSession {
	kind?: SessionKind;
	project?: string | null;
	note?: string;
	duration?: string;
}

export interface ProjectPatch {
	name?: string;
	color?: string | null;
	status?: ProjectStatus;
}

/** A non-2xx response, carrying the server's message so the UI can show it. */
export class ApiError extends Error {
	readonly status: number;

	constructor(status: number, message: string) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
	}
}

interface ErrorBody {
	error?: string;
}

async function request<T>(method: string, path: string, body?: JsonValue): Promise<T> {
	let response: Response;
	try {
		response = await fetch(path, {
			method,
			headers: body === undefined ? undefined : { 'content-type': 'application/json' },
			body: body === undefined ? undefined : JSON.stringify(body)
		});
	} catch {
		// A dropped connection is the common case on a phone that has wandered off
		// the tailnet; it deserves a clearer message than "Failed to fetch".
		throw new ApiError(0, 'Could not reach the server');
	}

	if (response.status === 204) {
		return undefined as T;
	}

	// The single boundary cast: past this point the payload is typed.
	const payload = (await response.json().catch(() => null)) as T & ErrorBody;

	if (!response.ok) {
		const message = payload?.error ?? `Request failed with ${response.status}`;
		throw new ApiError(response.status, message);
	}

	return payload;
}

export const api = {
	listProjects: (): Promise<Project[]> => request('GET', '/api/projects'),

	createProject: (project: NewProject): Promise<Project> =>
		request('POST', '/api/projects', { ...project }),

	readProject: (slug: string): Promise<Project> =>
		request('GET', `/api/projects/${encodeURIComponent(slug)}`),

	updateProject: (slug: string, patch: ProjectPatch): Promise<Project> =>
		request('PATCH', `/api/projects/${encodeURIComponent(slug)}`, { ...patch }),

	deleteProject: (slug: string): Promise<void> =>
		request('DELETE', `/api/projects/${encodeURIComponent(slug)}`),

	readTimer: (): Promise<TimerState> => request('GET', '/api/timer'),

	startSession: (session: StartSession): Promise<TimerState> =>
		request('POST', '/api/timer/start', { ...session }),

	stopSession: (): Promise<TimerState> => request('POST', '/api/timer/stop', {}),

	cancelSession: (): Promise<TimerState> => request('POST', '/api/timer/cancel', {})
};
