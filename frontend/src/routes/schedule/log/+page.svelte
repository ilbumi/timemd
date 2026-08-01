<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import PeriodHeader from '$lib/PeriodHeader.svelte';
	import { api, type LoggedSession, type Project } from '$lib/api';
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
	let bands = $state<Band[]>([]);
	let looks = $state<Record<string, Look>>({});
	let projects = $state<Project[]>([]);

	let error = $state<string | null>(null);
	let loading = $state(true);
	let adding = $state(false);

	let start = $state('09:00');
	let end = $state('10:00');
	let project = $state('');
	let note = $state('');

	const monday = $derived(startOfWeek(anchor));
	const totalMinutes = $derived(bands.reduce((total, band) => total + band.minutes, 0));
	const sessionCount = $derived(bands.reduce((total, band) => total + band.sessions.length, 0));

	/**
	 * The week's days, newest first, with the empty ones dropped.
	 *
	 * Seven reads rather than one endpoint because notes live in the day files and
	 * the report endpoint returns totals; they run in parallel, so it costs one
	 * round trip.
	 */
	async function load(): Promise<void> {
		error = await attempt(async () => {
			const days = await Promise.all(weekDates(monday).map((date) => api.readDay(date)));

			bands = days
				.map((day) => ({
					date: day.date,
					sessions: [...day.sessions].reverse(),
					minutes: parseMinutes(day.tracked)
				}))
				.filter((band) => band.sessions.length > 0)
				.reverse();
		});
		loading = false;
	}

	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(work);
	}

	const addSession = (event: SubmitEvent): Promise<void> => {
		event.preventDefault();
		return run(async () => {
			await api.addSession(today(), {
				start: `${start}:00`,
				end: `${end}:00`,
				project: project || null,
				note: note.trim()
			});
			note = '';
			adding = false;
			anchor = today();
			await load();
		});
	};

	const removeSession = (date: string, index: number): Promise<void> =>
		run(async () => {
			await api.deleteSession(date, index);
			await load();
		});

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
		total="{formatHours(totalMinutes)} · {sessionCount} sessions"
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
						<div class="entry">
							<div class="line">
								<span class="who">{look.name}</span>
								<span class="numeric when">
									{clockTime(session.start)} · {session.duration}
								</span>
							</div>
							<p class="note" class:none={session.note === ''}>{session.note || 'No note'}</p>
						</div>
						<button
							class="quiet danger"
							aria-label="Delete session"
							onclick={() => removeSession(band.date, session.index)}>×</button
						>
					</li>
				{/each}
			</ul>
		{/each}

		{#if adding}
			<form onsubmit={addSession}>
				<p class="meta">Logged against today.</p>
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
				<div class="actions">
					<button type="button" onclick={() => (adding = false)}>Cancel</button>
					<button class="primary" type="submit">Log it</button>
				</div>
			</form>
		{/if}
	</div>

	<div class="foot">
		<div class="actions">
			<button onclick={() => (adding = !adding)}>+ Time by hand</button>
		</div>
	</div>
</section>

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

	.entry {
		flex: 1;
		min-width: 0;
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

	form p {
		margin: 0;
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
