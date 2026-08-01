<script lang="ts">
	/**
	 * The header the three schedule views share: step back, a title, a total,
	 * step forward, and the segmented switch underneath.
	 *
	 * It was written out three times and had already drifted — one copy aligned
	 * its row on the baseline and dropped the arrows' centring. The title varies
	 * (a weekday, `WEEK 31`, `LOG`), so it comes in as a snippet.
	 */
	import ScheduleTabs from '$lib/ScheduleTabs.svelte';
	import type { Snippet } from 'svelte';

	let {
		unit,
		total,
		onPrevious,
		onNext,
		title
	}: {
		/** What the arrows step by, for the buttons' labels: "day" or "week". */
		unit: string;
		total: string;
		onPrevious: () => void;
		onNext: () => void;
		title: Snippet;
	} = $props();
</script>

<header>
	<div class="row">
		<button class="quiet" aria-label="Previous {unit}" onclick={onPrevious}>‹</button>
		<h1>{@render title()}</h1>
		<div class="total meta">{total}</div>
		<button class="quiet" aria-label="Next {unit}" onclick={onNext}>›</button>
	</div>
	<ScheduleTabs />
</header>

<style>
	header {
		padding: 14px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.row {
		display: flex;
		align-items: flex-end;
		gap: 8px;
		margin-bottom: 14px;
	}

	h1 {
		flex: 1;
		min-width: 0;
	}

	.total {
		text-align: right;
		text-transform: uppercase;
	}

	button {
		align-self: center;
		min-height: 0;
		font-size: 1.5rem;
		line-height: 1;
	}
</style>
