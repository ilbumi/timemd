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

	/* Grows into the free space but never shrinks below the title itself. With a
	   flex-basis of zero the total claimed its content width first and left the
	   title a box too small to sit in, which it then overflowed into the gap; the
	   basis fixes that and `0` shrink keeps it fixed as the total gets longer.
	   The total is left at its defaults, and so is the one that gives way — it
	   wraps, which is why a long title costs height here rather than a collision. */
	h1 {
		flex: 1 0 auto;
	}

	.total {
		text-align: right;
		text-transform: uppercase;
	}

	/* A thumb-sized target on a phone; a pointer needs less, and the row is
	   tighter for it. */
	button {
		align-self: center;
		min-width: var(--tap-target);
		padding: 0;
		font-size: 1.5rem;
		line-height: 1;
	}

	@media (min-width: 700px) {
		button {
			min-width: 32px;
			min-height: 32px;
		}
	}
</style>
