<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import PeriodHeader from '$lib/PeriodHeader.svelte';
	import { api, type DayView, type LoggedSession, type Project } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { formatHours, parseMinutes } from '$lib/countdown';
	import { clockTime, dayLabel, shiftDays, startOfWeek, today, weekDates } from '$lib/dates';
	import { lookOf, readLooks, type Look } from '$lib/look';

	interface Band {
		date: string;
		sessions: LoggedSession[];
		minutes: number;
	}

	let anchor = $state(today());
	let days = $state<DayView[]>([]);
	let looks = $state<Record<string, Look>>({});
	let projects = $state<Project[]>([]);

	let error = $state<string | null>(null);
	let loading = $state(true);
	/**
	 * The open form: adding at the foot, or amending one session inline. One
	 * value rather than an `adding` flag beside an `editing` target, because
	 * only one is ever open and nothing had to keep the pair consistent.
	 */
	let editor = $state<{ mode: 'add' } | { mode: 'amend'; date: string; index: number } | null>(
		null
	);

	let start = $state('09:00');
	let end = $state('10:00');
	let project = $state('');
	let note = $state('');
	let date = $state(today());

	const monday = $derived(startOfWeek(anchor));
	const week = $derived(weekDates(monday));

	/** The week's days, newest first, with the empty ones dropped. */
	const bands: Band[] = $derived(
		days
			.map((day) => ({
				date: day.date,
				sessions: [...day.sessions].reverse(),
				minutes: parseMinutes(day.tracked)
			}))
			.filter((band) => band.sessions.length > 0)
			.reverse()
	);
	const totalMinutes = $derived(bands.reduce((total, band) => total + band.minutes, 0));
	const sessionCount = $derived(bands.reduce((total, band) => total + band.sessions.length, 0));

	/**
	 * Off `days` rather than `bands`, which drops the empty ones: a day can be
	 * planned and have nothing tracked, and losing its plan would understate the
	 * week. The server's `duration` is used rather than end-minus-start because
	 * it is the one that survives a block crossing midnight.
	 */
	const plannedMinutes = $derived(
		days
			.flatMap((day) => day.planned)
			.reduce((total, block) => total + parseMinutes(block.duration), 0)
	);

	/**
	 * Seven reads rather than one endpoint because notes live in the day files and
	 * the report endpoint returns totals; they run in parallel, so it costs one
	 * round trip.
	 */
	async function load(): Promise<void> {
		error = await attempt(async () => {
			days = await Promise.all(week.map((each) => api.readDay(each)));
		});
		loading = false;
	}

	/**
	 * Runs an edit and re-reads the week.
	 *
	 * The re-read is in the wrapper rather than at each call site so a new
	 * mutation cannot forget it — and it is what covers the trap here: a day
	 * re-sorts around a changed start time, so an index held across a write may
	 * afterwards name a different session.
	 */
	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(async () => {
			await work();
			await load();
		});
	}

	/** The four fields both forms carry, in the shape the API takes. */
	const draft = () => ({
		start: `${start}:00`,
		end: `${end}:00`,
		project: project || null,
		note: note.trim()
	});

	/**
	 * Opens the form against the week on screen.
	 *
	 * Today when the shown week contains it, and its Monday otherwise: the
	 * header browses back through past weeks, and logging into one of them used
	 * to be impossible from here even though the endpoint is date-addressed.
	 */
	function openAdder(): void {
		date = week.includes(today()) ? today() : monday;
		editor = { mode: 'add' };
	}

	const addSession = (event: SubmitEvent): Promise<void> => {
		event.preventDefault();
		return run(async () => {
			await api.addSession(date, draft());
			note = '';
			editor = null;
		});
	};

	const removeSession = (date: string, index: number): Promise<void> =>
		run(() => api.deleteSession(date, index));

	/**
	 * Opens the editor on one session, pre-filled.
	 *
	 * Tapping the entry rather than a second trailing button: the row already
	 * ends in one, and two adjacent 44px reach-overlays pass the layout gate —
	 * it probes vertically — while still failing a thumb.
	 */
	function openEditor(date: string, session: LoggedSession): void {
		editor = { mode: 'amend', date, index: session.index };
		start = session.start.slice(0, 5);
		end = session.end.slice(0, 5);
		project = session.project ?? '';
		note = session.note;
	}

	const saveSession = (event: SubmitEvent): Promise<void> => {
		event.preventDefault();
		const target = editor;
		if (target?.mode !== 'amend') return Promise.resolve();

		return run(async () => {
			await api.updateSession(target.date, target.index, draft());
			editor = null;
		});
	};

	$effect(() => {
		void monday;
		void load();
	});

	$effect(() => {
		void readLooks().then((loaded) => {
			looks = loaded.looks;
			projects = loaded.active;
		});
	});
</script>

<section class="screen">
	<PeriodHeader
		unit="week"
		total="{formatHours(totalMinutes)} / {formatHours(plannedMinutes)} · {sessionCount} sessions"
		onPrevious={() => (anchor = shiftDays(anchor, -7))}
		onNext={() => (anchor = shiftDays(anchor, 7))}
	>
		{#snippet title()}LOG{/snippet}
	</PeriodHeader>

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<div class="body">
		{#if loading}
			<p class="empty">Loading…</p>
		{:else if bands.length === 0}
			<p class="empty">Nothing tracked this week.</p>
		{/if}

		{#each bands as band (band.date)}
			<div class="band">
				<span>{dayLabel(band.date)}</span>
				<span class="numeric">{formatHours(band.minutes)}</span>
			</div>

			<ul>
				{#each band.sessions as session (session.index)}
					{@const look = lookOf(looks, session.project)}
					<li>
						<Mark mark={look.mark} color={look.color} size={14} />
						<button
							class="entry"
							aria-label="Edit {look.name} at {clockTime(session.start)}"
							onclick={() => openEditor(band.date, session)}
						>
							<span class="line">
								<span class="who">{look.name}</span>
								<span class="numeric when">
									{clockTime(session.start)} · {session.duration}
								</span>
							</span>
							<span class="note" class:none={session.note === ''}>{session.note || 'No note'}</span>
						</button>
						<button
							class="quiet danger"
							aria-label="Delete session"
							onclick={() => removeSession(band.date, session.index)}>×</button
						>
					</li>

					{#if editor?.mode === 'amend' && editor.date === band.date && editor.index === session.index}
						<form onsubmit={saveSession}>
							{@render fields()}
							<div class="actions">
								<button type="button" onclick={() => (editor = null)}>Cancel</button>
								<button class="primary" type="submit">Save</button>
							</div>
						</form>
					{/if}
				{/each}
			</ul>
		{/each}

		{#if editor?.mode === 'add'}
			<form onsubmit={addSession}>
				<input
					type="date"
					aria-label="Date"
					min={week[0]}
					max={week[week.length - 1]}
					bind:value={date}
				/>
				{@render fields()}
				<div class="actions">
					<button type="button" onclick={() => (editor = null)}>Cancel</button>
					<button class="primary" type="submit">Log it</button>
				</div>
			</form>
		{/if}
	</div>

	<div class="foot">
		<div class="actions">
			<button onclick={() => (editor?.mode === 'add' ? (editor = null) : openAdder())}>
				+ Time by hand
			</button>
		</div>
	</div>
</section>

<!-- What a session is, in both the form that creates one and the form that
     amends one. Rendered twice so the two cannot drift apart. -->
{#snippet fields()}
	<div class="row">
		<input type="time" aria-label="Start" bind:value={start} />
		<input type="time" aria-label="End" bind:value={end} />
	</div>
	<select aria-label="Project" bind:value={project}>
		<option value="">No project</option>
		{#each projects as candidate (candidate.slug)}
			<option value={candidate.slug}>{candidate.name}</option>
		{/each}
	</select>
	<input type="text" placeholder="Note" aria-label="Note" bind:value={note} />
{/snippet}

<style>
	.body {
		flex: 1;
	}

	/* An inverted band per day: the log is the only screen that is mostly text,
	   so the days need something solid to break it up. */
	.band {
		display: flex;
		justify-content: space-between;
		gap: var(--gap);
		padding: 7px var(--pad);
		background: var(--ink);
		color: var(--paper);
		font-size: 0.6875rem;
		font-weight: 600;
		letter-spacing: 0.16em;
		text-transform: uppercase;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0 var(--pad);
	}

	li {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 13px 0;
		border-bottom: 1px solid var(--ink-15);
	}

	li :global(svg) {
		margin-top: 3px;
	}

	/*
	 * A button, so the whole entry opens the editor — but Chrome centres a
	 * button's contents, so it is a left-aligned flex column like the day
	 * screen's block text.
	 */
	.entry {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
	}

	.line {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 10px;
	}

	.who {
		font-size: 0.875rem;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.when {
		flex: none;
		font-size: 0.75rem;
		color: var(--ink-45);
	}

	.note {
		margin: 5px 0 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--ink-80);
	}

	.note.none {
		color: var(--ink-45);
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: var(--gap) var(--pad) 0;
	}

	.row {
		display: flex;
		gap: 8px;
	}

	.row > * {
		flex: 1;
		min-width: 0;
	}

	.foot {
		border-top: var(--rule) solid var(--ink);
	}

	/* ---- wide ------------------------------------------------------------ */

	@media (min-width: 700px) {
		/*
		 * The bands, the rows and the header all run to the same edge — capping
		 * the body left the header's rule and the foot's hanging 180px past
		 * everything between them. It is the note that needs a short line, not
		 * the screen, so the limit goes there.
		 */
		.note {
			max-width: 620px;
		}

		.foot > .actions {
			max-width: 320px;
		}
	}
</style>
