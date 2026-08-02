<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
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

	/**
	 * Read from the URL once, so the week view can link to a day.
	 *
	 * Only the initial value: the day arrows below move it without pushing
	 * history, which is the behaviour that was already here.
	 */
	let date = $state(page.url.searchParams.get('date') ?? today());
	let day = $state<DayView | null>(null);
	let allProjects = $state<Project[]>([]);
	let looks = $state<Record<string, Look>>({});
	let error = $state<string | null>(null);
	let loading = $state(true);
	let nowMinutes = $state(minutesNow());
	let adding = $state(false);
	/** The `oneOffIndex` being amended, or null when the form is creating one. */
	let editing = $state<number | null>(null);

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

	const span = $derived(
		// Stretched past the planned hours on today, so the now-bar does not
		// disappear off the bottom of the window in the evening.
		spanOf(planned, DEFAULT_FROM, isToday ? Math.max(DEFAULT_TO, nowMinutes + 30) : DEFAULT_TO)
	);
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

	/**
	 * Submits the block form, creating or amending depending on how it opened.
	 *
	 * The re-read in `mutate` covers the trap here: changing a start time
	 * re-sorts the day, so the `oneOffIndex` just used may name another block.
	 */
	const saveBlock = (event: SubmitEvent): Promise<void> => {
		event.preventDefault();
		const target = editing;
		return mutate(async () => {
			const block = {
				start: `${blockStart}:00`,
				end: `${blockEnd}:00`,
				project: blockProject || null,
				title: blockTitle.trim()
			};
			if (target === null) {
				await api.addBlock(date, block);
			} else {
				await api.updateBlock(date, target, block);
			}
			blockTitle = '';
			editing = null;
			adding = false;
		});
	};

	/** Opens the form on an existing one-off, pre-filled. */
	function openEditor(block: Occurrence, index: number): void {
		editing = index;
		blockStart = block.start.slice(0, 5);
		blockEnd = block.end.slice(0, 5);
		blockProject = block.project ?? '';
		blockTitle = block.title;
		adding = true;
	}

	function openAdder(): void {
		editing = null;
		blockTitle = '';
		adding = true;
	}

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
							style:top="{place.top}%"
							style:height="{place.height}%"
							style:--fill={done ? 'var(--paper)' : look.color}
							style:--edge={done ? look.color : 'var(--ink)'}
							style:--text={done ? 'var(--ink)' : contrastInk(look.color)}
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
						{#if block.oneOffIndex !== null}
							{@const index = block.oneOffIndex}
							<!-- The row opens the editor, so the one trailing button stays
							     the one destructive action. A second button beside it would
							     put two 44px reach-overlays side by side. -->
							<button
								class="what edit"
								aria-label="Edit {block.title || look.name}"
								onclick={() => openEditor(block, index)}
							>
								<span class="numeric when">{clockTime(block.start)}</span>
								<span class="title">{block.title || look.name}</span>
							</button>
							<button class="quiet danger" onclick={() => removeBlock(index)}>Remove</button>
						{:else}
							<span class="numeric when">{clockTime(block.start)}</span>
							<span class="what">{block.title || look.name}</span>
							{#if block.block}
								{@const id = block.block}
								<button class="quiet" onclick={() => skip(id)}>Skip</button>
							{/if}
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
			<form class="add" onsubmit={saveBlock}>
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
					<button type="button" onclick={() => ((adding = false), (editing = null))}>Cancel</button>
					<button class="primary" type="submit">
						{editing === null ? 'Add block' : 'Save block'}
					</button>
				</div>
			</form>
		{/if}
	</div>

	<div class="foot">
		<div class="actions">
			<button onclick={() => (adding ? ((adding = false), (editing = null)) : openAdder())}>
				+ Block
			</button>
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
		height: clamp(320px, 46dvh, 620px);
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

	/*
	 * Pulled left by one rule so the block's own border lands *on* the lane's
	 * axis instead of beside it. Flush, they drew a 4px double line down the
	 * left of every block with a notched corner, while the right edge was a
	 * single 2px rule — the block looked lopsided.
	 */
	.block {
		position: absolute;
		left: calc(-1 * var(--rule));
		right: 0;
		display: block;
		min-height: 0;
		/* So a block too short for two lines can drop the second one. */
		container-type: size;
		padding: 5px 11px;
		overflow: hidden;
		/*
		 * Fill, edge and text arrive as custom properties rather than as inline
		 * `background`/`color`, so a state like `:hover` can still override them.
		 * Set inline directly they won the cascade, which left an outline as the
		 * only way to show hover — and that drew a second line inside the border.
		 *
		 * They are also what draws a finished block rather than filling it — the
		 * same way an archived project's mark is drawn. Fading it instead mixed
		 * the fill, the text and the lane's rule showing through into one muddy
		 * colour, and nothing else in this design is a tint.
		 */
		background: var(--fill);
		border: var(--rule) solid var(--edge);
		color: var(--text);
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
		gap: 2px;
	}

	/* Two lines need about 44px. Below that the time goes and the title stays,
	   which is the half worth keeping. */
	@container (max-height: 43px) {
		.block-when {
			display: none;
		}
	}

	.block-title {
		font-size: 0.84375rem;
		font-weight: 600;
		line-height: 1.15;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.block-when {
		font-size: 0.6875rem;
		line-height: 1.15;
		opacity: 0.8;
		white-space: nowrap;
	}

	/*
	 * The only red on the screen, which is the point of it. Kept inside the lane
	 * rather than reaching back into the gutter: out there it crossed the hour
	 * labels, and an evening reading of "20" came with a line through it.
	 */
	.now {
		position: absolute;
		left: 0;
		right: 0;
		height: 2px;
		background: var(--red);
	}

	/* Sits against the axis rather than straddling it, so nothing crosses into
	   the gutter at all. */
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

	/*
	 * The row opens the editor. Left-aligned for the same reason `.block-text`
	 * is a column: Chrome centres a button's contents itself.
	 */
	.edit {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 44px;
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
	}

	.edit .title {
		min-width: 0;
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

	@media (min-width: 700px) {
		.timeline {
			height: clamp(360px, 52dvh, 620px);
		}

		/* The bar keeps its full-width rule; only the buttons are capped. */
		.foot > .actions {
			max-width: 520px;
		}
	}

	@container screen (min-width: 900px) {
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
			height: clamp(360px, 58dvh, 700px);
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
	}

	@media (hover: hover) {
		/*
		 * Inverts rather than gaining a ring: another line inside the border is
		 * exactly the doubling this screen just lost.
		 *
		 * Sets the properties, not the variables — the variables arrive inline,
		 * and inline wins over a stylesheet whatever the selector. What this rule
		 * competes with is the base `background: var(--fill)` beside it, which it
		 * beats on specificity.
		 */
		.block:hover {
			background: var(--ink);
			border-color: var(--ink);
			color: var(--paper);
		}
	}
</style>
