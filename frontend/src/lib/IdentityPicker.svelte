<script lang="ts">
	/**
	 * Shape and colour, chosen independently.
	 *
	 * Shared by creating, editing, and first-run, which is the whole reason it
	 * is a component: the screens must offer the same five marks and the same
	 * five colours, or a project could be given a pairing on creation that it
	 * could never be edited back to. A file-seeded blue circle is a valid
	 * project; the picker has to be able to make one too.
	 */
	import Mark from '$lib/Mark.svelte';
	import type { Mark as MarkShape } from '$lib/api';
	import { DEFAULT_COLOR, MARKS, PALETTE, PALETTE_NAMES } from '$lib/palette';

	let {
		mark = $bindable('square'),
		color = $bindable(DEFAULT_COLOR)
	}: { mark: MarkShape; color: string } = $props();
</script>

<div class="picker">
	<span class="label">Mark</span>
	<div class="row" role="radiogroup" aria-label="Mark">
		{#each MARKS as shape (shape)}
			{@const chosen = shape === mark}
			<button
				type="button"
				role="radio"
				aria-checked={chosen}
				aria-label={shape}
				onclick={() => (mark = shape)}
			>
				<Mark mark={shape} color="var(--ink)" size={20} />
				{#if chosen}<span class="chosen"></span>{/if}
			</button>
		{/each}
	</div>

	<span class="label">Colour</span>
	<div class="row" role="radiogroup" aria-label="Colour">
		{#each PALETTE as swatch, index (swatch)}
			{@const chosen = swatch === color}
			<button
				type="button"
				role="radio"
				aria-checked={chosen}
				aria-label={PALETTE_NAMES[index] ?? swatch}
				style:background={swatch}
				onclick={() => (color = swatch)}
			>
				{#if chosen}<span class="chosen"></span>{/if}
			</button>
		{/each}
	</div>
</div>

<style>
	/*
	 * The selection rule hangs 9px below the swatches and is absolutely
	 * positioned, so it takes no space of its own. The padding gives it some:
	 * without it the bar landed on whatever followed the picker — in the project
	 * detail's edit form, right on top of the "Weekly target" label.
	 */
	.picker {
		display: flex;
		flex-direction: column;
		gap: 10px;
		color: inherit;
	}

	.picker :global(.label) {
		color: inherit;
	}

	.row {
		display: flex;
		gap: 9px;
		padding-bottom: 9px;
	}

	button {
		position: relative;
		flex: 1;
		max-width: 60px;
		aspect-ratio: 1;
		min-height: 0;
		display: grid;
		place-items: center;
		padding: 0;
		background: var(--white);
	}

	/* The design marks the selection with a rule under the swatch rather than a
	   ring around it, so the swatch's own colour stays uninterrupted. */
	.chosen {
		position: absolute;
		left: 0;
		right: 0;
		bottom: -9px;
		height: 5px;
		background: var(--ink);
	}
</style>
