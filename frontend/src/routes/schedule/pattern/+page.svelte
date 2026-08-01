<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import { ApiError, api, type Project, type RecurringBlock } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { clockTime } from '$lib/dates';
	import { contrastInk } from '$lib/palette';
	import { lookOf, looksFrom, type Look } from '$lib/look';

	const DAYS = [
		{ key: 'mon', letter: 'M' },
		{ key: 'tue', letter: 'T' },
		{ key: 'wed', letter: 'W' },
		{ key: 'thu', letter: 'T' },
		{ key: 'fri', letter: 'F' },
		{ key: 'sat', letter: 'S' },
		{ key: 'sun', letter: 'S' }
	];

	/** The three lead times the design offers, plus off. */
	const LEADS = [
		{ value: '0m', label: 'At time' },
		{ value: '10m', label: '10 min' },
		{ value: '30m', label: '30 min' }
	];

	let blocks = $state<RecurringBlock[]>([]);
	let projects = $state<Project[]>([]);
	let looks = $state<Record<string, Look>>({});
	let error = $state<string | null>(null);
	let loading = $state(true);
	let dirty = $state(false);

	const blank = (): RecurringBlock => ({
		id: '',
		days: 'mon-fri',
		start: '09:00:00',
		end: '10:00:00',
		project: null,
		title: '',
		remindBefore: '10m'
	});

	/**
	 * The day spec as a set of weekdays.
	 *
	 * The stored form is the grammar's — `mon-fri`, `mon,wed,fri`, `daily` — and
	 * the editor's is seven squares, so both directions live here rather than
	 * being re-derived per square.
	 */
	function selectedDays(spec: string): Set<string> {
		const trimmed = spec.trim().toLowerCase();
		if (trimmed === 'daily') return new Set(DAYS.map((day) => day.key));

		const chosen = new Set<string>();
		for (const part of trimmed.split(',')) {
			const [from, to] = part.split('-');
			const first = DAYS.findIndex((day) => day.key === from?.trim());
			if (first === -1) continue;
			const last = to === undefined ? first : DAYS.findIndex((day) => day.key === to.trim());
			for (let index = first; index <= (last === -1 ? first : last); index += 1) {
				const day = DAYS[index];
				if (day !== undefined) chosen.add(day.key);
			}
		}
		return chosen;
	}

	/** Back to the grammar. Always the explicit comma list: it is what the
	    grammar accepts for every combination, ranges only for some. */
	function toSpec(chosen: Set<string>): string {
		if (chosen.size === DAYS.length) return 'daily';
		return DAYS.filter((day) => chosen.has(day.key))
			.map((day) => day.key)
			.join(',');
	}

	function toggleDay(block: RecurringBlock, key: string): void {
		const chosen = selectedDays(block.days);
		if (chosen.has(key)) {
			chosen.delete(key);
		} else {
			chosen.add(key);
		}
		// An empty spec would never fire; keep the last day rather than silently
		// producing a block that does nothing.
		if (chosen.size > 0) block.days = toSpec(chosen);
		dirty = true;
	}

	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(work);
	}

	async function load(): Promise<void> {
		await run(async () => {
			blocks = await api.readRecurring();
		});
		loading = false;
	}

	const add = (): void => {
		blocks = [...blocks, blank()];
		dirty = true;
	};

	const remove = (position: number): void => {
		blocks = blocks.filter((_, index) => index !== position);
		dirty = true;
	};

	const save = (): Promise<void> =>
		run(async () => {
			// Blocks with no id would be dropped by the server anyway; refusing here
			// gives a clearer message than a validation error from the grammar.
			const unnamed = blocks.findIndex((block) => block.id.trim() === '');
			if (unnamed !== -1) {
				throw new ApiError(400, `Block ${unnamed + 1} needs an id`);
			}
			blocks = await api.writeRecurring(blocks);
			dirty = false;
		});

	$effect(() => {
		void load();
		api
			.listProjects()
			.then((all) => {
				projects = all.filter((project) => project.status === 'active');
				looks = looksFrom(all);
			})
			.catch(() => {
				// The project row just shows fewer choices.
			});
	});
</script>

<section class="screen">
	<header class="bar">
		<a class="close" href="/schedule/week" aria-label="Back">←</a>
		<span class="eyebrow">Pattern</span>
		<button class="quiet" onclick={save} disabled={!dirty}>Save</button>
	</header>

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<div class="body">
		{#if loading}
			<p class="empty">Loading…</p>
		{:else if blocks.length === 0}
			<p class="empty">
				No repeating blocks yet. These live in <code>schedule/recurring.md</code>, one line each.
			</p>
		{/if}

		{#each blocks as block, position (position)}
			{@const look = lookOf(looks, block.project)}
			<article>
				<div class="row">
					<input
						type="text"
						placeholder="id"
						aria-label="Block id for row {position + 1}"
						bind:value={block.id}
						oninput={() => (dirty = true)}
					/>
					<button
						class="quiet danger"
						aria-label="Delete block {position + 1}"
						onclick={() => remove(position)}>Delete</button
					>
				</div>

				<input
					type="text"
					placeholder="Title"
					aria-label="Title for row {position + 1}"
					bind:value={block.title}
					oninput={() => (dirty = true)}
				/>

				<div class="pair">
					<label>
						<span class="label">Start</span>
						<input
							type="time"
							value={clockTime(block.start)}
							onchange={(event) => {
								block.start = `${event.currentTarget.value}:00`;
								dirty = true;
							}}
						/>
					</label>
					<label>
						<span class="label">End</span>
						<input
							type="time"
							value={clockTime(block.end)}
							onchange={(event) => {
								block.end = `${event.currentTarget.value}:00`;
								dirty = true;
							}}
						/>
					</label>
				</div>

				<div class="field">
					<span class="label" id="repeats-{position}">Repeats</span>
					<div class="days" role="group" aria-labelledby="repeats-{position}">
						{#each DAYS as day, index (day.key)}
							{@const on = selectedDays(block.days).has(day.key)}
							<button
								class="day"
								aria-pressed={on}
								aria-label={day.key}
								style:background={on ? look.color : 'transparent'}
								style:color={on ? contrastInk(look.color) : 'var(--ink-45)'}
								style:border-color={on ? 'var(--ink)' : 'var(--ink-30)'}
								onclick={() => toggleDay(block, day.key)}
							>
								{DAYS[index]?.letter}
							</button>
						{/each}
					</div>
				</div>

				<div class="field">
					<span class="label" id="project-{position}">Project</span>
					<div class="projects" role="group" aria-labelledby="project-{position}">
						<button
							class="pick"
							aria-pressed={block.project === null}
							onclick={() => {
								block.project = null;
								dirty = true;
							}}
						>
							None
						</button>
						{#each projects as candidate (candidate.slug)}
							{@const chosen = block.project === candidate.slug}
							{@const candidateLook = lookOf(looks, candidate.slug)}
							<button
								class="pick"
								aria-pressed={chosen}
								style:background={chosen ? candidateLook.color : 'transparent'}
								style:color={chosen ? contrastInk(candidateLook.color) : 'var(--ink)'}
								onclick={() => {
									block.project = candidate.slug;
									dirty = true;
								}}
							>
								<Mark
									mark={candidateLook.mark}
									color={chosen ? contrastInk(candidateLook.color) : candidateLook.color}
									size={14}
								/>
								{candidate.name}
							</button>
						{/each}
					</div>
				</div>

				<div class="field">
					<div class="switch">
						<span>Push reminder</span>
						<button
							class="toggle"
							role="switch"
							aria-checked={block.remindBefore !== null}
							aria-label="Push reminder for block {position + 1}"
							onclick={() => {
								block.remindBefore = block.remindBefore === null ? '10m' : null;
								dirty = true;
							}}
						>
							<span></span>
						</button>
					</div>

					{#if block.remindBefore !== null}
						<div class="segmented">
							{#each LEADS as lead (lead.value)}
								<button
									aria-pressed={block.remindBefore === lead.value}
									onclick={() => {
										block.remindBefore = lead.value;
										dirty = true;
									}}
								>
									{lead.label}
								</button>
							{/each}
						</div>
					{/if}
				</div>
			</article>
		{/each}
	</div>

	<div class="foot">
		<div class="actions">
			<button onclick={add}>+ Repeating block</button>
			<button class="primary" onclick={save} disabled={!dirty}>Save pattern</button>
		</div>
	</div>
</section>

<style>
	.screen {
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}

	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--gap);
		padding: 14px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.close {
		font-size: 1.25rem;
		line-height: 1;
		text-decoration: none;
	}

	.body {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	article {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 18px var(--pad);
		border-bottom: var(--rule) solid var(--ink);
	}

	.row {
		display: flex;
		gap: 8px;
	}

	.row input {
		flex: 1;
		min-width: 0;
	}

	.pair {
		display: flex;
		gap: 12px;
	}

	.pair label {
		flex: 1;
		min-width: 0;
		margin: 0;
	}

	.pair .label {
		display: block;
		margin-bottom: 8px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.days {
		display: flex;
		gap: 6px;
	}

	.day {
		flex: 1;
		aspect-ratio: 1;
		min-height: 0;
		padding: 0;
		font-size: 0.75rem;
		font-weight: 600;
		letter-spacing: 0;
	}

	.projects {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.pick {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 40px;
		padding: 0 11px;
		font-size: 0.8125rem;
		font-weight: 500;
		letter-spacing: 0.04em;
		text-transform: none;
	}

	.pick[aria-pressed='false'] {
		border-color: var(--ink-30);
	}

	.switch {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		font-size: 0.9375rem;
	}

	/* A hard-edged switch: the knob is a black square that slides, not a pill. */
	.toggle {
		flex: none;
		width: 46px;
		height: 26px;
		min-height: 0;
		padding: 0;
		position: relative;
		background: var(--paper);
	}

	.toggle[aria-checked='true'] {
		background: var(--yellow);
	}

	.toggle > span {
		position: absolute;
		top: 0;
		bottom: 0;
		left: 0;
		width: 22px;
		background: var(--ink-30);
	}

	.toggle[aria-checked='true'] > span {
		left: auto;
		right: 0;
		background: var(--ink);
	}

	.foot {
		padding: 13px var(--pad) 16px;
		border-top: var(--rule) solid var(--ink);
	}
</style>
