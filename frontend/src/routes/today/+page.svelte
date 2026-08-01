<script lang="ts">
	import { api, type DayView, type Project } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { clockTime, dayLabel, shiftDays, today } from '$lib/dates';

	let date = $state(today());
	let day = $state<DayView | null>(null);
	let projects = $state<Project[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);

	let addingSession = $state(false);
	let start = $state('09:00');
	let end = $state('10:00');
	let project = $state('');
	let note = $state('');

	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(work);
	}

	async function load(): Promise<void> {
		await run(async () => {
			day = await api.readDay(date);
		});
		loading = false;
	}

	const move = (days: number): void => {
		date = shiftDays(date, days);
	};

	async function addSession(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		await run(async () => {
			await api.addSession(date, {
				start: `${start}:00`,
				end: `${end}:00`,
				project: project || null,
				note: note.trim()
			});
			note = '';
			addingSession = false;
			day = await api.readDay(date);
		});
	}

	const removeSession = (index: number): Promise<void> =>
		run(async () => {
			await api.deleteSession(date, index);
			day = await api.readDay(date);
		});

	const toggleSkip = (id: string, skipped: boolean): Promise<void> =>
		run(async () => {
			await (skipped ? api.unskipBlock(date, id) : api.skipBlock(date, id));
			day = await api.readDay(date);
		});

	const removeBlock = (index: number): Promise<void> =>
		run(async () => {
			await api.deleteBlock(date, index);
			day = await api.readDay(date);
		});

	$effect(() => {
		// Re-runs whenever `date` changes, which is what drives the day arrows.
		void date;
		void load();
	});

	$effect(() => {
		api
			.listActiveProjects()
			.then((active) => {
				projects = active;
			})
			.catch(() => {
				// The day is readable without the project list.
			});
	});
</script>

<header>
	<button class="quiet" aria-label="Previous day" onclick={() => move(-1)}>‹</button>
	<div>
		<h1>{dayLabel(date)}</h1>
		<p class="muted">
			{date}{#if day}
				· {day.tracked} tracked{/if}
		</p>
	</div>
	<button class="quiet" aria-label="Next day" onclick={() => move(1)}>›</button>
</header>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}

{#if loading}
	<p class="muted">Loading…</p>
{:else if day}
	{@const current = day}

	{#if current.problems.length > 0}
		<div class="problems" role="status">
			<strong>{current.problems.length} line(s) in this file could not be read</strong>
			<ul>
				{#each current.problems as problem (problem)}
					<li>{problem}</li>
				{/each}
			</ul>
			<p>They are kept as written and moved to the end of their section.</p>
		</div>
	{/if}

	<section>
		<h2>Planned</h2>
		{#if current.planned.length === 0}
			<p class="muted">Nothing scheduled.</p>
		{:else}
			<ul>
				{#each current.planned as block, position (`${block.start}-${block.title}-${position}`)}
					<li>
						<span class="when">{clockTime(block.start)}–{clockTime(block.end)}</span>
						<span class="what">
							{#if block.project}<span class="tag">{block.project}</span>{/if}
							{block.title}
						</span>
						{#if block.block}
							{@const id = block.block}
							<button class="quiet" onclick={() => toggleSkip(id, false)}>Skip</button>
						{:else if block.oneOffIndex !== null}
							{@const index = block.oneOffIndex}
							<button class="quiet danger" onclick={() => removeBlock(index)}>Remove</button>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}

		{#each current.skipped as id (id)}
			<p class="skipped">
				<span>{id} skipped</span>
				<button class="quiet" onclick={() => toggleSkip(id, true)}>Restore</button>
			</p>
		{/each}
	</section>

	<section>
		<h2>Tracked</h2>
		{#if current.sessions.length === 0}
			<p class="muted">Nothing logged yet.</p>
		{:else}
			<ul>
				{#each current.sessions as session (session.index)}
					<li>
						<span class="when">{clockTime(session.start)}–{clockTime(session.end)}</span>
						<span class="what">
							{#if session.project}<span class="tag">{session.project}</span>{/if}
							{session.note}
						</span>
						<span class="muted">{session.duration}</span>
						<button
							class="quiet danger"
							aria-label="Delete session"
							onclick={() => removeSession(session.index)}>×</button
						>
					</li>
				{/each}
			</ul>
		{/if}

		{#if addingSession}
			<form onsubmit={addSession}>
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
				<div class="row">
					<button class="primary" type="submit">Add</button>
					<button type="button" onclick={() => (addingSession = false)}>Cancel</button>
				</div>
			</form>
		{:else}
			<button onclick={() => (addingSession = true)}>Add time by hand</button>
		{/if}
	</section>
{/if}

<style>
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		margin-bottom: var(--gap);
	}

	header div {
		flex: 1;
		text-align: center;
	}

	header button {
		font-size: 1.5rem;
		line-height: 1;
		padding: 0 12px;
	}

	section {
		margin-bottom: 20px;
	}

	h2 {
		margin-bottom: 8px;
	}

	ul {
		list-style: none;
		margin: 0 0 8px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	li {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px;
		background: var(--surface-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
	}

	.when {
		font-variant-numeric: tabular-nums;
		font-size: 0.85rem;
		color: var(--text-muted);
		white-space: nowrap;
	}

	.what {
		flex: 1;
		min-width: 0;
	}

	.tag {
		display: inline-block;
		padding: 1px 7px;
		margin-right: 4px;
		border-radius: 999px;
		background: var(--surface-sunken);
		font-size: 0.75rem;
	}

	.skipped {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		margin: 0 0 6px;
		color: var(--text-muted);
		font-size: 0.85rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.row {
		display: flex;
		gap: 8px;
	}

	.row > * {
		flex: 1;
	}

	.problems {
		padding: 10px 12px;
		margin-bottom: var(--gap);
		border-radius: var(--radius);
		background: color-mix(in srgb, var(--danger) 12%, transparent);
		color: var(--danger);
	}

	.problems ul {
		margin: 6px 0;
		padding-left: 18px;
		list-style: disc;
	}

	.problems li {
		display: list-item;
		background: none;
		border: none;
		padding: 0;
		font-size: 0.85rem;
	}

	.problems p {
		margin: 0;
		font-size: 0.85rem;
	}
</style>
