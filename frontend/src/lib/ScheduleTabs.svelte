<script lang="ts">
	/**
	 * The schedule's three segments.
	 *
	 * They navigate rather than switch a local variable, so each view is a real
	 * URL and loads only its own data — a week raster is seven day reads that the
	 * log has no use for.
	 */
	import { page } from '$app/state';

	const SEGMENTS = [
		{ href: '/schedule', label: 'Day' },
		{ href: '/schedule/week', label: 'Week' },
		{ href: '/schedule/log', label: 'Log' }
	];
</script>

<nav class="segmented" aria-label="Schedule view">
	{#each SEGMENTS as segment (segment.href)}
		{@const current = page.url.pathname === segment.href}
		<a href={segment.href} aria-current={current ? 'page' : undefined}>{segment.label}</a>
	{/each}
</nav>

<style>
	/*
	 * The sheet's `.segmented` already supplies the box, the type and the rule
	 * between segments, and inverts the current one. All that is left is
	 * centring a link's label the way a button's is centred for free.
	 */
	.segmented a {
		display: flex;
		align-items: center;
		justify-content: center;
		text-decoration: none;
	}
</style>
