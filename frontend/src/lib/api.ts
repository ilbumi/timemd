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
	milestones?: Milestone[];
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
	/** Work on a todo: its project and description fill in for the two above. */
	todo?: string;
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
	/** Todos scheduled for this day, untimed first. Read-only here. */
	todos: Todo[];
	problems: string[];
}

export type TodoStatus = 'open' | 'done' | 'cancelled';

export type Priority = 'highest' | 'high' | 'medium' | 'normal' | 'low' | 'lowest';

/**
 * One todo, in the same spelling `todos.md` uses.
 *
 * Dates are `YYYY-MM-DD`, optionally with a ` HH:MM`. They stay strings rather
 * than becoming `Date`s: the files carry no offsets, and parsing them into an
 * instant is exactly the conversion that would put one back.
 */
export interface Todo {
	/** Null only for a hand-written todo the app has not written yet. */
	id: string | null;
	/** `open`, `done`, `cancelled`, or a single character somebody chose. */
	status: string;
	description: string;
	project: string | null;
	priority: Priority;
	tags: string[];
	recurrence: string | null;
	dependsOn: string[];
	created: string | null;
	start: string | null;
	scheduled: string | null;
	due: string | null;
	cancelled: string | null;
	done: string | null;
	onCompletion: string | null;
}

export interface TodoList {
	todos: Todo[];
	/** Lines the server could not read, kept and reported. */
	problems: string[];
}

export interface NewTodo {
	description: string;
	project?: string | null;
	priority?: Priority;
	scheduled?: string | null;
	due?: string | null;
	start?: string | null;
	recurrence?: string | null;
}

/** An absent field means "leave it"; `null` means "clear it". */
export interface TodoPatch {
	description?: string;
	status?: string;
	project?: string | null;
	priority?: Priority;
	scheduled?: string | null;
	due?: string | null;
	start?: string | null;
	done?: string | null;
	cancelled?: string | null;
	recurrence?: string | null;
}

/** Narrows `listTodos`. Every field absent means every todo. */
export interface TodoFilter {
	project?: string;
	status?: TodoStatus;
	dueBefore?: string;
	scheduledOn?: string;
}

/**
 * Declared as a type alias rather than an interface so it is assignable to
 * `JsonValue` when sent as a request body: TypeScript gives object type aliases
 * an implicit index signature, and interfaces none. That keeps `writeRecurring`
 * honest instead of casting through `unknown`.
 */
export type RecurringBlock = {
	id: string;
	/** Weekday names, Monday first. The server spells the stored form. */
	days: string[];
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
	/** What the schedule set aside for this key over the range. */
	planned: string;
	sessions: number;
}

export interface Report {
	from: string;
	to: string;
	groupBy: GroupBy;
	total: string;
	/** Everything scheduled over the range; `total` is what was tracked. */
	planned: string;
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

/** What a test send did, and why the setup might not be working. */
export type NtfyTest = 'delivered' | 'rejected' | 'unreachable';

export interface Ntfy {
	server: string;
	/** Null means the channel is off. */
	topic: string | null;
	appUrl: string | null;
	/** Whether a token is set. The value itself never leaves the server. */
	hasToken: boolean;
	/** What a phone subscribes to, or null while the channel is off. */
	subscribeUrl: string | null;
	/** Non-null only on a write that moved where notifications go. */
	test: NtfyTest | null;
}

/**
 * Every field is optional and every one but `server` accepts an explicit null
 * to clear it — sending `undefined` would leave the value alone instead, which
 * is how "turn it off" becomes a silent no-op.
 */
export interface NtfyPatch {
	server?: string;
	topic?: string | null;
	token?: string | null;
	appUrl?: string | null;
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

	listTodos: (filter: TodoFilter = {}): Promise<TodoList> => {
		const query = new URLSearchParams();
		if (filter.project !== undefined) query.set('project', filter.project);
		if (filter.status !== undefined) query.set('status', filter.status);
		if (filter.dueBefore !== undefined) query.set('dueBefore', filter.dueBefore);
		if (filter.scheduledOn !== undefined) query.set('scheduledOn', filter.scheduledOn);
		const suffix = query.size === 0 ? '' : `?${query}`;
		return request('GET', `/api/todos${suffix}`);
	},

	createTodo: (todo: NewTodo): Promise<Todo> => request('POST', '/api/todos', { ...todo }),

	/** Edits one todo. Unlike milestones there is no whole-list write: a todo
	    has an id, so naming the one row that changed cannot clobber another. */
	updateTodo: (id: string, patch: TodoPatch): Promise<Todo> =>
		request('PATCH', `/api/todos/${encodeURIComponent(id)}`, { ...patch }),

	deleteTodo: (id: string): Promise<void> =>
		request('DELETE', `/api/todos/${encodeURIComponent(id)}`),

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

	updateBlock: (date: string, index: number, block: BlockEdit): Promise<void> =>
		request('PATCH', `/api/days/${date}/blocks/${index}`, { ...block }),

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

	readNtfy: (): Promise<Ntfy> => request('GET', '/api/ntfy'),

	writeNtfy: (patch: NtfyPatch): Promise<Ntfy> => request('PUT', '/api/ntfy', { ...patch }),

	pushKey: (): Promise<PushKey> => request('GET', '/api/push/key'),

	subscribePush: (subscription: PushSubscriptionInput): Promise<void> =>
		request('POST', '/api/push/subscribe', { ...subscription }),

	unsubscribePush: (endpoint: string): Promise<void> =>
		request('DELETE', '/api/push/subscribe', { endpoint })
};
