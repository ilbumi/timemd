<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import { ApiError, api, type Project, type RecurringBlock } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { clockTime } from '$lib/dates';
	import { contrastInk } from '$lib/palette';
	import { lookOf, readLooks, type Look } from '$lib/look';

	const DAYS = [
		{ key: 'mon', letter: 'M' },
		{ key: 'tue', letter: 'T' },
		{ key: 'wed', letter: 'W' },
		{ key: 'thu', letter: 'T' },
		{ key: 'fri', letter: 'F' },
		{ key: 'sat', letter: 'S' },
		{ key: 'sun', letter: 'S' }
	];

	/** The lead times the design offers. A block whose stored lead is none of
	    them keeps it as a fourth choice rather than losing it on the next tap. */
	const LEADS = [
		{ value: '0m', label: 'At time' },
		{ value: '10m', label: '10 min' },
		{ value: '30m', label: '30 min' }
	];

	function leadsFor(lead: string | null): { value: string; label: string }[] {
		if (lead === null || LEADS.some((option) => option.value === lead)) return LEADS;
		return [...LEADS, { value: lead, label: lead }];
	}

	let blocks = $state<RecurringBlock[]>([]);
	let projects = $state<Project[]>([]);
	let looks = $state<Record<string, Look>>({});
	let error = $state<string | null>(null);
	let loading = $state(true);
	let dirty = $state(false);

	const blank = (): RecurringBlock => ({
		id: '',
		days: ['mon', 'tue', 'wed', 'thu', 'fri'],
		start: '09:00:00',
		end: '10:00:00',
		project: null,
		title: '',
		remindBefore: '10m'
	});

	/**
	 * Adds or removes one weekday.
	 *
	 * The wire carries a plain list of names and the server spells the stored
	 * form, so this screen never has to know that `mon-fri` and `daily` exist.
	 */
	function toggleDay(block: RecurringBlock, key: string): void {
		const next = block.days.includes(key)
			? block.days.filter((day) => day !== key)
			: DAYS.filter((day) => day.key === key || block.days.includes(day.key)).map((day) => day.key);
		// A block on no days would be refused by the server and would never fire;
		// keep the last one rather than letting the user reach that state.
		if (next.length > 0) block.days = next;
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
		void readLooks().then((loaded) => {
			looks = loaded.looks;
			projects = loaded.active;
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
						{#each DAYS as day (day.key)}
							{@const on = block.days.includes(day.key)}
							<button
								class="day"
								aria-pressed={on}
								aria-label={day.key}
								style:background={on ? look.color : 'transparent'}
								style:color={on ? contrastInk(look.color) : 'var(--ink-45)'}
								style:border-color={on ? 'var(--ink)' : 'var(--ink-30)'}
								onclick={() => toggleDay(block, day.key)}
							>
								{day.letter}
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
							{#each leadsFor(block.remindBefore) as lead (lead.value)}
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
	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--gap);
		padding: 14px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.close {
		display: flex;
		align-items: center;
		justify-content: center;
		width: var(--tap-target);
		height: var(--tap-target);
		margin-left: -12px;
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

	/*
	 * The switch stays 26px tall because that is what the design draws; the thumb
	 * gets its 44px from an invisible overlay instead. `inset` is measured from
	 * the padding box, so with the 2px rule the reach either side is 11px, not 9.
	 */
	.toggle::after {
		content: '';
		position: absolute;
		inset: -11px 0;
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

	/* ---- wide ------------------------------------------------------------ */

	@media (min-width: 700px) {
		/* The rule between two blocks ends where the fields do. */
		.screen {
			max-width: 620px;
		}

		/* Seven squares across 620px would be 80px each — bigger than they need
		   to be, and the row stops reading as a week. */
		.day {
			max-width: 64px;
		}

		.foot > .actions {
			max-width: 400px;
		}
	}
</style>
