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
	.segmented a {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 34px;
		font-size: 0.75rem;
		font-weight: 400;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		text-decoration: none;
	}

	.segmented a + a {
		border-left: var(--rule) solid var(--ink);
	}

	.segmented a[aria-current='page'] {
		background: var(--ink);
		color: var(--paper);
		font-weight: 600;
	}
</style>
