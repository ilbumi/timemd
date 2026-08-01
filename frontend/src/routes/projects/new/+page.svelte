<script lang="ts">
	import { goto } from '$app/navigation';
	import IdentityPicker from '$lib/IdentityPicker.svelte';
	import Mark from '$lib/Mark.svelte';
	import { api, type Mark as MarkShape, type Milestone } from '$lib/api';
	import { describe } from '$lib/attempt';
	import { formatHours } from '$lib/countdown';
	import { DEFAULT_COLOR } from '$lib/palette';

	/** The stepper's bounds, in minutes. A week has 168 hours; anything near it
	    is a typo. Half-hour steps so `1h30m` is expressible. */
	const MAX_TARGET = 60 * 60;
	const TARGET_STEP = 30;

	let name = $state('');
	let mark = $state<MarkShape>('square');
	let color = $state(DEFAULT_COLOR);
	let target = $state(10 * 60);
	let milestones = $state<Milestone[]>([]);
	let draft = $state('');
	let error = $state<string | null>(null);
	let busy = $state(false);

	function step(by: number): void {
		target = Math.min(MAX_TARGET, Math.max(0, target + by));
	}

	function addMilestone(): void {
		const title = draft.trim();
		if (title === '') return;
		milestones = [...milestones, { done: false, title }];
		draft = '';
	}

	function removeMilestone(position: number): void {
		milestones = milestones.filter((_, index) => index !== position);
	}

	async function create(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (name.trim() === '' || busy) return;

		busy = true;
		error = null;
		try {
			const created = await api.createProject({
				name: name.trim(),
				color,
				mark,
				target: target === 0 ? null : `${target}m`,
				milestones
			});
			await goto(`/projects/${created.slug}`);
		} catch (failure) {
			error = describe(failure);
		} finally {
			busy = false;
		}
	}
</script>

<form class="screen" onsubmit={create}>
	<header class="bar">
		<a class="close" href="/projects" aria-label="Cancel">×</a>
		<span class="eyebrow">New project</span>
		<span class="close" aria-hidden="true"></span>
	</header>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	<div class="body">
		<div class="field">
			<label class="label" for="name">Name</label>
			<input id="name" class="underlined" type="text" placeholder="Thesis" bind:value={name} />
		</div>

		<div class="field">
			<span class="label">Mark</span>
			<IdentityPicker bind:mark bind:color />
		</div>

		<div class="field">
			<span class="label" id="target-label">Weekly target</span>
			<div class="stepper" role="group" aria-labelledby="target-label">
				<button type="button" onclick={() => step(-TARGET_STEP)} aria-label="Less time">−</button>
				<span class="reading">
					<strong class="numeric">{target === 0 ? '—' : formatHours(target)}</strong>
					<span>{target === 0 ? 'no target' : 'per week'}</span>
				</span>
				<button
					type="button"
					class="accent"
					onclick={() => step(TARGET_STEP)}
					aria-label="More time"
				>
					+
				</button>
			</div>
		</div>

		<div class="field">
			<span class="label">Milestones</span>
			{#each milestones as milestone, position (position)}
				<div class="milestone">
					<Mark mark="triangle" color="var(--ink)" size={16} outline />
					<span>{milestone.title}</span>
					<button
						type="button"
						class="quiet"
						aria-label="Remove {milestone.title}"
						onclick={() => removeMilestone(position)}>×</button
					>
				</div>
			{/each}

			<div class="milestone">
				<Mark mark="triangle" color="var(--ink-30)" size={16} outline />
				<input
					type="text"
					class="draft"
					placeholder="Add a milestone…"
					aria-label="New milestone"
					bind:value={draft}
					onkeydown={(event) => {
						if (event.key === 'Enter') {
							event.preventDefault();
							addMilestone();
						}
					}}
				/>
				<button type="button" class="quiet" onclick={addMilestone} disabled={draft.trim() === ''}>
					Add
				</button>
			</div>
		</div>
	</div>

	<div class="foot">
		<div class="actions">
			<button class="primary" type="submit" disabled={busy || name.trim() === ''}>Create</button>
		</div>
	</div>
</form>

<style>
	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--gap);
		padding: 14px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	/* The glyph sits at the padding edge rather than centred in its tap box, so
	   it lines up with the fields below instead of hanging outside them. The
	   spacer opposite shares the class, so the eyebrow between them stays
	   centred. */
	.close {
		display: flex;
		align-items: center;
		width: var(--tap-target);
		height: var(--tap-target);
		font-size: 1.25rem;
		line-height: 1;
		text-decoration: none;
	}

	.body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 22px;
		padding: 20px var(--pad) 0;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.stepper {
		display: flex;
		align-items: stretch;
		border: var(--rule) solid var(--ink);
	}

	.stepper > button {
		flex: none;
		width: 54px;
		border: none;
		font-size: 1.625rem;
		font-weight: 300;
	}

	.stepper > button:first-child {
		border-right: var(--rule) solid var(--ink);
	}

	.stepper > button:last-child {
		border-left: var(--rule) solid var(--ink);
	}

	.reading {
		flex: 1;
		display: flex;
		align-items: baseline;
		justify-content: center;
		gap: 6px;
		padding: 13px 0;
	}

	.reading strong {
		font-size: 1.75rem;
		font-weight: 500;
		line-height: 1;
	}

	.reading span {
		font-size: 0.8125rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--ink-60);
	}

	.milestone {
		display: flex;
		align-items: center;
		gap: 11px;
		padding-bottom: 9px;
		border-bottom: 1px solid var(--ink-15);
		font-size: 0.875rem;
	}

	.milestone > span {
		flex: 1;
		min-width: 0;
	}

	.draft {
		flex: 1;
		min-height: 32px;
		padding: 0;
		border: none;
		background: none;
	}

	.foot {
		padding: 12px var(--pad) 16px;
	}

	/* ---- wide ------------------------------------------------------------ */

	@media (min-width: 700px) {
		/* A form does not get better by getting wider — and the bar above it
		   should not either, or its rule hangs out past the fields. */
		.screen {
			max-width: 560px;
		}
	}
</style>
