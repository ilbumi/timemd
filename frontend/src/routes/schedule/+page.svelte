<script lang="ts">
	import { goto } from '$app/navigation';
	import Mark from '$lib/Mark.svelte';
	import ScheduleTabs from '$lib/ScheduleTabs.svelte';
	import { api, type DayView, type Occurrence, type Project } from '$lib/api';
	import { attempt, describe } from '$lib/attempt';
	import { formatHours } from '$lib/countdown';
	import { clockTime, minutesOfDay, shiftDays, today } from '$lib/dates';
	import { contrastInk } from '$lib/palette';
	import { lookOf, looksFrom, type Look } from '$lib/look';

	/** The window the timeline always shows, widened to fit anything outside it. */
	const DEFAULT_FROM = 8 * 60;
	const DEFAULT_TO = 20 * 60;
	/** How often the now-bar moves. A minute is as fine as it needs to be. */
	const NOW_MS = 60_000;

	let date = $state(today());
	let day = $state<DayView | null>(null);
	let looks = $state<Record<string, Look>>({});
	let projects = $state<Project[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let nowMinutes = $state(currentMinutes());
	let adding = $state(false);

	let blockStart = $state('09:00');
	let blockEnd = $state('10:00');
	let blockProject = $state('');
	let blockTitle = $state('');

	function currentMinutes(): number {
		const now = new Date();
		return now.getHours() * 60 + now.getMinutes();
	}

	const planned = $derived(day?.planned ?? []);
	const isToday = $derived(date === today());

	const plannedMinutes = $derived(
		planned.reduce(
			(total, block) => total + (minutesOfDay(block.end) - minutesOfDay(block.start)),
			0
		)
	);

	/** The visible span, stretched to hold every block on the day. Not named
	    `window`: shadowing the global inside a component is asking for it. */
	const span = $derived.by(() => {
		const starts = planned.map((block) => minutesOfDay(block.start));
		const ends = planned.map((block) => minutesOfDay(block.end));
		const from = Math.min(DEFAULT_FROM, ...starts);
		const to = Math.max(DEFAULT_TO, ...ends);
		return { from, to: Math.max(to, from + 60) };
	});

	const hours = $derived.by(() => {
		const marks: number[] = [];
		// Every second hour, which is what fits the gutter at phone width.
		for (let hour = Math.ceil(span.from / 60); hour * 60 <= span.to; hour += 2) {
			marks.push(hour);
		}
		return marks;
	});

	function offset(minutes: number): number {
		return ((minutes - span.from) / (span.to - span.from)) * 100;
	}

	function height(block: Occurrence): number {
		return offset(minutesOfDay(block.end)) - offset(minutesOfDay(block.start));
	}

	/** A block counts as done once a logged session overlaps it. */
	function isDone(block: Occurrence): boolean {
		const from = minutesOfDay(block.start);
		const to = minutesOfDay(block.end);
		return (day?.sessions ?? []).some(
			(session) => minutesOfDay(session.start) < to && minutesOfDay(session.end) > from
		);
	}

	/** The block to offer on the start button: the one running now, else the next. */
	const upcoming = $derived(planned.find((block) => minutesOfDay(block.end) > nowMinutes) ?? null);

	async function load(): Promise<void> {
		error = await attempt(async () => {
			day = await api.readDay(date);
		});
		loading = false;
	}

	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(work);
	}

	const addBlock = (event: SubmitEvent): Promise<void> => {
		event.preventDefault();
		return run(async () => {
			await api.addBlock(date, {
				start: `${blockStart}:00`,
				end: `${blockEnd}:00`,
				project: blockProject || null,
				title: blockTitle.trim()
			});
			blockTitle = '';
			adding = false;
			day = await api.readDay(date);
		});
	};

	const skip = (id: string): Promise<void> =>
		run(async () => {
			await api.skipBlock(date, id);
			day = await api.readDay(date);
		});

	const unskip = (id: string): Promise<void> =>
		run(async () => {
			await api.unskipBlock(date, id);
			day = await api.readDay(date);
		});

	const removeBlock = (index: number): Promise<void> =>
		run(async () => {
			await api.deleteBlock(date, index);
			day = await api.readDay(date);
		});

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
		api
			.listProjects()
			.then((all) => {
				projects = all.filter((project) => project.status === 'active');
				looks = looksFrom(all);
			})
			.catch(() => {
				// Blocks fall back to a derived colour without it.
			});

		const tick = setInterval(() => {
			nowMinutes = currentMinutes();
		}, NOW_MS);
		return () => clearInterval(tick);
	});
</script>

<section class="screen">
	<header class="head">
		<div class="head-top">
			<button class="quiet" aria-label="Previous day" onclick={() => (date = shiftDays(date, -1))}>
				‹
			</button>
			<h1>
				{new Date(`${date}T00:00`).toLocaleDateString(undefined, { weekday: 'long' })}<br />
				<span class="light"
					>{new Date(`${date}T00:00`).toLocaleDateString(undefined, {
						day: 'numeric',
						month: 'short'
					})}</span
				>
			</h1>
			<div class="totals meta">
				{formatHours(plannedMinutes)}<br />planned
			</div>
			<button class="quiet" aria-label="Next day" onclick={() => (date = shiftDays(date, 1))}>
				›
			</button>
		</div>
		<ScheduleTabs />
	</header>

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
						<span style:top="{offset(hour * 60)}%">{hour.toString().padStart(2, '0')}</span>
					{/each}
				</div>

				<div class="lane">
					{#each planned as block, position (`${block.start}-${block.title}-${position}`)}
						{@const look = lookOf(looks, block.project)}
						{@const done = isDone(block)}
						<button
							class="block"
							class:done
							style:top="{offset(minutesOfDay(block.start))}%"
							style:height="{height(block)}%"
							style:background={look.color}
							style:color={contrastInk(look.color)}
							onclick={() => startBlock(block)}
						>
							<span class="block-title">{block.title || look.name}</span>
							<span class="block-when">
								{clockTime(block.start)}–{clockTime(block.end)}{done ? ' · done' : ''}
							</span>
						</button>
					{/each}

					{#if isToday && nowMinutes >= span.from && nowMinutes <= span.to}
						<div class="now" style:top="{offset(nowMinutes)}%" aria-hidden="true">
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
	.screen {
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}

	.head {
		padding: 14px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.head-top {
		display: flex;
		align-items: flex-end;
		gap: 8px;
		margin-bottom: 14px;
	}

	.head-top h1 {
		flex: 1;
		text-transform: uppercase;
	}

	.light {
		font-weight: 300;
	}

	.totals {
		text-align: right;
		text-transform: uppercase;
	}

	.head-top button {
		font-size: 1.5rem;
		line-height: 1;
		min-height: 0;
		align-self: center;
	}

	.canvas {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	.timeline {
		display: flex;
		height: 320px;
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
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 3px;
		min-height: 0;
		padding: 6px 11px;
		overflow: hidden;
		border: var(--rule) solid var(--ink);
		text-align: left;
		text-transform: none;
		letter-spacing: 0;
	}

	.block.done {
		opacity: 0.35;
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
		padding: 13px var(--pad) 16px;
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
</style>
