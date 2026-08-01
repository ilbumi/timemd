<script lang="ts">
	import { page } from '$app/state';
	import Mark from '$lib/Mark.svelte';
	import type { Mark as MarkShape } from '$lib/api';
	import '../app.css';

	let { children } = $props();

	/**
	 * Three tabs, drawn as the three basic shapes: circle is time, square is a
	 * project, triangle is a plan. Settings and the log are not tabs — settings
	 * hangs off the timer's header, the log is a segment of the schedule — which
	 * is what keeps the bar down to three thumb-sized targets.
	 */
	const tabs: { href: string; label: string; mark: MarkShape }[] = [
		{ href: '/', label: 'Timer', mark: 'circle' },
		{ href: '/projects', label: 'Projects', mark: 'square' },
		{ href: '/schedule', label: 'Schedule', mark: 'triangle' }
	];

	function isCurrent(href: string): boolean {
		return href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
	}
</script>

<div class="shell">
	<main>
		{@render children()}
	</main>

	<nav aria-label="Sections">
		{#each tabs as tab (tab.href)}
			{@const current = isCurrent(tab.href)}
			<a href={tab.href} aria-label={tab.label} aria-current={current ? 'page' : undefined}>
				<Mark mark={tab.mark} size={22} color={current ? 'var(--paper)' : 'var(--ink)'} />
			</a>
		{/each}
	</nav>
</div>

<style>
	.shell {
		display: flex;
		flex-direction: column;
		height: 100dvh;
		max-width: 440px;
		margin: 0 auto;
		background: var(--paper);
	}

	main {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		padding-top: env(safe-area-inset-top);
	}

	nav {
		flex: none;
		display: flex;
		border-top: var(--rule) solid var(--ink);
		padding-bottom: env(safe-area-inset-bottom);
	}

	nav a {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 58px;
	}

	nav a + a {
		border-left: var(--rule) solid var(--ink);
	}

	nav a[aria-current='page'] {
		background: var(--ink);
	}
</style>
