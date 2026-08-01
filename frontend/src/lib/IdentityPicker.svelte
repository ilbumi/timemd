<script lang="ts">
	/**
	 * The shape × colour grid from the new-project screen.
	 *
	 * Shared by creating and editing a project, which is the whole reason it is a
	 * component: the two screens must offer the same five identities or a project
	 * could be given one on creation that it could never be edited back to.
	 */
	import Mark from '$lib/Mark.svelte';
	import type { Mark as MarkShape } from '$lib/api';
	import { IDENTITIES, contrastInk } from '$lib/palette';

	let {
		mark = $bindable('square'),
		color = $bindable(IDENTITIES[0]?.color ?? '#245a8d')
	}: { mark: MarkShape; color: string } = $props();
</script>

<div class="picker" role="radiogroup" aria-label="Mark">
	{#each IDENTITIES as identity (identity.mark)}
		{@const chosen = identity.mark === mark}
		<button
			type="button"
			role="radio"
			aria-checked={chosen}
			aria-label={identity.mark}
			style:background={identity.color}
			onclick={() => {
				mark = identity.mark;
				color = identity.color;
			}}
		>
			<Mark mark={identity.mark} color={contrastInk(identity.color)} size={20} />
			{#if chosen}<span class="chosen"></span>{/if}
		</button>
	{/each}
</div>

<style>
	.picker {
		display: flex;
		gap: 9px;
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
