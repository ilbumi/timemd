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

/** The geometric shape a project is drawn as. Mirrors core's `Mark`. */
export type Mark = 'square' | 'circle' | 'triangle' | 'diamond' | 'bar';

/** A type alias, not an interface, for the same reason as `RecurringBlock`
    below: it is sent as a request body and needs an implicit index signature. */
export type Milestone = {
	done: boolean;
	title: string;
};

export interface Project {
	slug: string;
	name: string;
	color: string | null;
	mark: Mark;
	/** Weekly hour target as a duration (`10h`), or null for none. */
	target: string | null;
	status: ProjectStatus;
	created: string | null;
	milestones: Milestone[];
	/** Milestone lines the server could not read, kept and reported. */
	problems: string[];
}

export interface NewProject {
	name: string;
	color?: string | null;
	mark?: Mark;
	target?: string | null;
}

export type SessionKind = 'focus' | 'short_break' | 'long_break';

export interface Running {
	kind: SessionKind;
	project: string | null;
	note: string;
	startedAt: string;
	endsAt: string;
	duration: string;
	/** The block's full length in seconds, for the dial. */
	durationSeconds: number;
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

export interface Occurrence {
	date: string;
	start: string;
	end: string;
	duration: string;
	project: string | null;
	title: string;
	remindBefore: string | null;
	/** The repeating block this came from, or null for a one-off. */
	block: string | null;
	/** Position among the day's one-offs, which is what deletion addresses. */
	oneOffIndex: number | null;
}

export interface LoggedSession {
	index: number;
	start: string;
	end: string;
	duration: string;
	project: string | null;
	note: string;
}

export interface DayView {
	date: string;
	sessions: LoggedSession[];
	tracked: string;
	planned: Occurrence[];
	skipped: string[];
	problems: string[];
}

/**
 * Declared as a type alias rather than an interface so it is assignable to
 * `JsonValue` when sent as a request body: TypeScript gives object type aliases
 * an implicit index signature, and interfaces none. That keeps `writeRecurring`
 * honest instead of casting through `unknown`.
 */
export type RecurringBlock = {
	id: string;
	days: string;
	start: string;
	end: string;
	project: string | null;
	title: string;
	remindBefore: string | null;
};

export interface SessionEdit {
	start: string;
	end: string;
	project?: string | null;
	note?: string;
}

export interface BlockEdit {
	start: string;
	end: string;
	project?: string | null;
	title?: string;
	remindBefore?: string | null;
}

export type GroupBy = 'project' | 'day';

export interface Bucket {
	/** Project slug or date, depending on the grouping; null means no project. */
	key: string | null;
	tracked: string;
	sessions: number;
}

export interface Report {
	from: string;
	to: string;
	groupBy: GroupBy;
	total: string;
	buckets: Bucket[];
}

export interface Settings {
	timezone: string;
	focus: string;
	shortBreak: string;
	longBreak: string;
	longBreakEvery: number;
	remindBefore: string;
}

/** The four knobs the settings screen edits. Timezone is read-only on purpose. */
export interface SettingsPatch {
	focus?: string;
	shortBreak?: string;
	longBreak?: string;
	remindBefore?: string;
}

export interface PushKey {
	publicKey: string;
}

export interface PushSubscriptionInput {
	endpoint: string;
	p256dh: string;
	auth: string;
}

export interface ProjectPatch {
	name?: string;
	color?: string | null;
	mark?: Mark;
	target?: string | null;
	status?: ProjectStatus;
	/** Replaces the whole list when given. */
	milestones?: Milestone[];
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

	/** Projects you can start a session against. */
	listActiveProjects: async (): Promise<Project[]> =>
		(await request<Project[]>('GET', '/api/projects')).filter(
			(project) => project.status === 'active'
		),

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

	cancelSession: (): Promise<TimerState> => request('POST', '/api/timer/cancel', {}),

	readDay: (date: string): Promise<DayView> => request('GET', `/api/days/${date}`),

	addSession: (date: string, session: SessionEdit): Promise<void> =>
		request('POST', `/api/days/${date}/sessions`, { ...session }),

	updateSession: (date: string, index: number, session: SessionEdit): Promise<void> =>
		request('PATCH', `/api/days/${date}/sessions/${index}`, { ...session }),

	deleteSession: (date: string, index: number): Promise<void> =>
		request('DELETE', `/api/days/${date}/sessions/${index}`),

	addBlock: (date: string, block: BlockEdit): Promise<void> =>
		request('POST', `/api/days/${date}/blocks`, { ...block }),

	deleteBlock: (date: string, index: number): Promise<void> =>
		request('DELETE', `/api/days/${date}/blocks/${index}`),

	skipBlock: (date: string, id: string): Promise<void> =>
		request('POST', `/api/days/${date}/skips`, { id }),

	unskipBlock: (date: string, id: string): Promise<void> =>
		request('DELETE', `/api/days/${date}/skips/${encodeURIComponent(id)}`),

	readSchedule: (from: string, to: string): Promise<Occurrence[]> =>
		request('GET', `/api/schedule?from=${from}&to=${to}`),

	readRecurring: (): Promise<RecurringBlock[]> => request('GET', '/api/schedule/recurring'),

	writeRecurring: (blocks: RecurringBlock[]): Promise<RecurringBlock[]> =>
		request('PUT', '/api/schedule/recurring', blocks),

	readReport: (from: string, to: string, groupBy: GroupBy): Promise<Report> =>
		request('GET', `/api/reports?from=${from}&to=${to}&groupBy=${groupBy}`),

	readSettings: (): Promise<Settings> => request('GET', '/api/settings'),

	writeSettings: (patch: SettingsPatch): Promise<Settings> =>
		request('PUT', '/api/settings', { ...patch }),

	pushKey: (): Promise<PushKey> => request('GET', '/api/push/key'),

	subscribePush: (subscription: PushSubscriptionInput): Promise<void> =>
		request('POST', '/api/push/subscribe', { ...subscription }),

	unsubscribePush: (endpoint: string): Promise<void> =>
		request('DELETE', '/api/push/subscribe', { endpoint })
};
