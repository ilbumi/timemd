<script lang="ts">
	import { page } from '$app/state';
	import '../app.css';

	let { children } = $props();

	/**
	 * Tabs appear as their screens are built. Kept as data so the bar has one
	 * definition rather than five hand-written links.
	 */
	const tabs = [
		{ href: '/', label: 'Timer', icon: 'M12 7v5l3 2' },
		{ href: '/projects', label: 'Projects', icon: 'M3 8h18M3 8l2-3h5l2 3M3 8v9h18V8' }
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
			<a href={tab.href} aria-current={isCurrent(tab.href) ? 'page' : undefined}>
				<svg viewBox="0 0 24 24" aria-hidden="true">
					{#if tab.href === '/'}
						<circle cx="12" cy="12" r="9" />
					{/if}
					<path d={tab.icon} />
				</svg>
				<span>{tab.label}</span>
			</a>
		{/each}
	</nav>
</div>

<style>
	.shell {
		display: flex;
		flex-direction: column;
		min-height: 100dvh;
	}

	main {
		flex: 1;
		padding: max(16px, env(safe-area-inset-top)) 16px 16px;
		padding-left: max(16px, env(safe-area-inset-left));
		padding-right: max(16px, env(safe-area-inset-right));
		max-width: 640px;
		width: 100%;
		margin: 0 auto;
	}

	nav {
		position: sticky;
		bottom: 0;
		display: flex;
		justify-content: space-around;
		gap: 4px;
		padding: 6px 8px calc(6px + env(safe-area-inset-bottom));
		background: var(--surface-raised);
		border-top: 1px solid var(--border);
	}

	nav a {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
		min-height: var(--tap-target);
		padding: 4px;
		border-radius: var(--radius);
		text-decoration: none;
		color: var(--text-muted);
		font-size: 0.72rem;
	}

	nav a[aria-current='page'] {
		color: var(--accent);
		background: var(--surface-sunken);
	}

	svg {
		width: 22px;
		height: 22px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.8;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
