<script lang="ts">
	import { goto } from '$app/navigation';
	import Mark from '$lib/Mark.svelte';
	import PeriodHeader from '$lib/PeriodHeader.svelte';
	import { api, type DayView, type Occurrence, type Project } from '$lib/api';
	import { attempt, describe } from '$lib/attempt';
	import { formatHours } from '$lib/countdown';
	import { clockTime, minutesOfDay, monthDay, shiftDays, today, weekdayName } from '$lib/dates';
	import { contrastInk } from '$lib/palette';
	import { lookOf, readLooks, type Look } from '$lib/look';
	import { hourMarks, minutesNow, offsetIn, placeIn, spanOf } from '$lib/timeline';

	/** The hours the timeline always shows, widened to fit anything outside them. */
	const DEFAULT_FROM = 8 * 60;
	const DEFAULT_TO = 20 * 60;
	/** Every second hour, which is what fits the gutter at phone width. */
	const HOUR_STEP = 2;
	/** How often the now-bar moves. A minute is as fine as it needs to be. */
	const NOW_MS = 60_000;

	let date = $state(today());
	let day = $state<DayView | null>(null);
	let allProjects = $state<Project[]>([]);
	let looks = $state<Record<string, Look>>({});
	let error = $state<string | null>(null);
	let loading = $state(true);
	let nowMinutes = $state(minutesNow());
	let adding = $state(false);

	let blockStart = $state('09:00');
	let blockEnd = $state('10:00');
	let blockProject = $state('');
	let blockTitle = $state('');

	const planned = $derived(day?.planned ?? []);
	const isToday = $derived(date === today());
	const projects = $derived(allProjects.filter((project) => project.status === 'active'));

	const plannedMinutes = $derived(
		planned.reduce(
			(total, block) => total + (minutesOfDay(block.end) - minutesOfDay(block.start)),
			0
		)
	);

	const span = $derived(spanOf(planned, DEFAULT_FROM, DEFAULT_TO));
	const hours = $derived(hourMarks(span, HOUR_STEP));

	/** The day's sessions as minute ranges, parsed once rather than once per
	    block: `isDone` is otherwise O(blocks x sessions) string parses. */
	const tracked = $derived(
		(day?.sessions ?? []).map((session) => ({
			from: minutesOfDay(session.start),
			to: minutesOfDay(session.end)
		}))
	);

	/** A block counts as done once a logged session overlaps it. */
	function isDone(block: Occurrence): boolean {
		const from = minutesOfDay(block.start);
		const to = minutesOfDay(block.end);
		return tracked.some((session) => session.from < to && session.to > from);
	}

	/** The block to offer on the start button: the one running now, else the next. */
	const upcoming = $derived(planned.find((block) => minutesOfDay(block.end) > nowMinutes) ?? null);

	async function load(): Promise<void> {
		error = await attempt(async () => {
			day = await api.readDay(date);
		});
		loading = false;
	}

	/**
	 * Runs an edit and re-reads the day.
	 *
	 * The re-read is in the wrapper rather than at each call site so a new
	 * mutation cannot forget it and leave the timeline silently stale.
	 */
	async function mutate(work: () => Promise<void>): Promise<void> {
		error = await attempt(async () => {
			await work();
			day = await api.readDay(date);
		});
	}

	const addBlock = (event: SubmitEvent): Promise<void> => {
		event.preventDefault();
		return mutate(async () => {
			await api.addBlock(date, {
				start: `${blockStart}:00`,
				end: `${blockEnd}:00`,
				project: blockProject || null,
				title: blockTitle.trim()
			});
			blockTitle = '';
			adding = false;
		});
	};

	const skip = (id: string): Promise<void> => mutate(() => api.skipBlock(date, id));

	const unskip = (id: string): Promise<void> => mutate(() => api.unskipBlock(date, id));

	const removeBlock = (index: number): Promise<void> => mutate(() => api.deleteBlock(date, index));

	async function startBlock(block: Occurrence): Promise<void> {
		try {
			await api.startSession({ kind: 'focus', project: block.project, note: block.title });
			await goto('/');
		} catch (failure) {
			error = describe(failure);
		}
	}

	$effect(() => {
		// Re-runs whenever `date` changes, which is what drives the day arrows.
		void date;
		void load();
	});

	$effect(() => {
		void readLooks().then((loaded) => {
			looks = loaded.looks;
			allProjects = loaded.active;
		});

		const tick = setInterval(() => {
			nowMinutes = minutesNow();
		}, NOW_MS);
		return () => clearInterval(tick);
	});
</script>

<section class="screen">
	<PeriodHeader
		unit="day"
		total="{formatHours(plannedMinutes)} planned"
		onPrevious={() => (date = shiftDays(date, -1))}
		onNext={() => (date = shiftDays(date, 1))}
	>
		{#snippet title()}
			{weekdayName(date)}<br /><span class="light">{monthDay(date)}</span>
		{/snippet}
	</PeriodHeader>

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	{#if day && day.problems.length > 0}
		<div class="problems" role="status">
			<strong>{day.problems.length} line(s) in this file could not be read</strong>
			<ul>
				{#each day.problems as problem (problem)}
					<li>{problem}</li>
				{/each}
			</ul>
		</div>
	{/if}

	<div class="canvas">
		{#if loading}
			<p class="empty">Loading…</p>
		{:else if planned.length === 0}
			<p class="empty">
				Nothing planned. Repeating blocks live in <a href="/schedule/pattern">the pattern</a>; a
				one-off goes below.
			</p>
		{:else}
			<div class="timeline">
				<div class="gutter">
					{#each hours as hour (hour)}
						<span style:top="{offsetIn(span, hour * 60)}%">
							{hour.toString().padStart(2, '0')}
						</span>
					{/each}
				</div>

				<div class="lane">
					{#each planned as block, position (`${block.start}-${block.title}-${position}`)}
						{@const look = lookOf(looks, block.project)}
						{@const done = isDone(block)}
						{@const place = placeIn(span, block)}
						<button
							class="block"
							class:done
							style:top="{place.top}%"
							style:height="{place.height}%"
							style:background={look.color}
							style:color={done ? 'var(--ink)' : contrastInk(look.color)}
							onclick={() => startBlock(block)}
						>
							<span class="block-text">
								<span class="block-title">{block.title || look.name}</span>
								<span class="block-when">
									{clockTime(block.start)}–{clockTime(block.end)}{done ? ' · done' : ''}
								</span>
							</span>
						</button>
					{/each}

					{#if isToday && nowMinutes >= span.from && nowMinutes <= span.to}
						<div class="now" style:top="{offsetIn(span, nowMinutes)}%" aria-hidden="true">
							<span class="dot"></span>
						</div>
					{/if}
				</div>
			</div>

			<ul class="legend">
				{#each planned as block, position (`${block.start}-${block.title}-${position}`)}
					{@const look = lookOf(looks, block.project)}
					<li>
						<Mark mark={look.mark} color={look.color} size={13} />
						<span class="numeric when">{clockTime(block.start)}</span>
						<span class="what">{block.title || look.name}</span>
						{#if block.block}
							{@const id = block.block}
							<button class="quiet" onclick={() => skip(id)}>Skip</button>
						{:else if block.oneOffIndex !== null}
							{@const index = block.oneOffIndex}
							<button class="quiet danger" onclick={() => removeBlock(index)}>Remove</button>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}

		{#each day?.skipped ?? [] as id (id)}
			<p class="skipped">
				<span><code>{id}</code> skipped today</span>
				<button class="quiet" onclick={() => unskip(id)}>Restore</button>
			</p>
		{/each}

		{#if adding}
			<form class="add" onsubmit={addBlock}>
				<div class="row">
					<input type="time" aria-label="Start" bind:value={blockStart} />
					<input type="time" aria-label="End" bind:value={blockEnd} />
				</div>
				<select aria-label="Project" bind:value={blockProject}>
					<option value="">No project</option>
					{#each projects as project (project.slug)}
						<option value={project.slug}>{project.name}</option>
					{/each}
				</select>
				<input type="text" placeholder="Title" aria-label="Title" bind:value={blockTitle} />
				<div class="actions">
					<button type="button" onclick={() => (adding = false)}>Cancel</button>
					<button class="primary" type="submit">Add block</button>
				</div>
			</form>
		{/if}
	</div>

	<div class="foot">
		<div class="actions">
			<button onclick={() => (adding = !adding)}>+ Block</button>
			{#if upcoming}
				{@const block = upcoming}
				{@const look = lookOf(looks, block.project)}
				<button class="accent grow" onclick={() => startBlock(block)}>
					<Mark mark={look.mark} color={look.color} size={12} />
					Start {clockTime(block.start)}
				</button>
			{/if}
		</div>
	</div>
</section>

<style>
	.light {
		font-weight: 300;
	}

	.canvas {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	.timeline {
		display: flex;
		height: clamp(320px, 46vh, 620px);
		padding: 14px var(--pad) 0;
	}

	.gutter {
		position: relative;
		flex: none;
		width: 30px;
	}

	.gutter span {
		position: absolute;
		left: 0;
		transform: translateY(-50%);
		font-size: 0.65625rem;
		color: var(--ink-45);
	}

	.lane {
		position: relative;
		flex: 1;
		border-left: var(--rule) solid var(--ink);
	}

	.block {
		position: absolute;
		left: 0;
		right: 0;
		display: block;
		min-height: 0;
		padding: 6px 11px;
		overflow: hidden;
		border: var(--rule) solid var(--ink);
		text-align: left;
		text-transform: none;
		letter-spacing: 0;
	}

	/*
	 * The text is its own column rather than the button being one: Chrome centres
	 * a button's contents itself, so a block shorter than two lines clipped the
	 * title and kept the time — the wrong way round.
	 */
	.block-text {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	/* A finished block is drawn faded, which is why its text goes back to ink:
	   paper on a 35%-opacity fill is unreadable. */
	.block.done {
		opacity: 0.4;
		border-style: none;
	}

	.block-title {
		font-size: 0.84375rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.block-when {
		font-size: 0.6875rem;
		opacity: 0.8;
		white-space: nowrap;
	}

	/* The only red on the screen, which is the point of it. */
	.now {
		position: absolute;
		left: -36px;
		right: 0;
		height: 2px;
		background: var(--red);
	}

	.dot {
		position: absolute;
		left: 0;
		top: -3.5px;
		width: 9px;
		height: 9px;
		border-radius: 50%;
		background: var(--red);
	}

	.legend {
		list-style: none;
		margin: 18px 0 0;
		padding: 0 var(--pad);
	}

	.legend li {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 9px 0;
		border-bottom: 1px solid var(--ink-15);
	}

	.when {
		flex: none;
		font-size: 0.75rem;
		color: var(--ink-45);
	}

	.what {
		flex: 1;
		min-width: 0;
		font-size: 0.84375rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.skipped {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		margin: 0;
		padding: 9px var(--pad);
		font-size: 0.8125rem;
		color: var(--ink-60);
	}

	.add {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin: var(--gap) var(--pad) 0;
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

	.grow {
		flex: 1.5 !important;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 9px;
		letter-spacing: 0.12em;
	}

	/* ---- wide ------------------------------------------------------------ */

	@media (min-width: 900px) {
		/* Timeline beside its own list, rather than above it: the day is short
		   enough that scrolling past the blocks to read them was the only reason
		   they were stacked. */
		.canvas {
			display: grid;
			grid-template-columns: 1fr 340px;
			gap: 0 var(--pad);
			align-content: start;
		}

		.canvas > :global(p) {
			grid-column: 1 / -1;
		}

		.timeline {
			height: clamp(360px, 58vh, 700px);
			padding-right: 0;
		}

		.legend {
			margin-top: 14px;
			padding-left: 0;
			padding-right: var(--pad);
		}

		.skipped,
		.add {
			grid-column: 1 / -1;
		}

		/* The bar keeps its full-width rule; only the buttons are capped. */
		.foot > .actions {
			max-width: 520px;
		}
	}

	@media (hover: hover) {
		.block:hover {
			outline: var(--rule) solid var(--ink);
			outline-offset: -2px;
		}
	}
</style>
