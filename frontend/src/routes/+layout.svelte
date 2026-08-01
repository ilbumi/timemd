<script lang="ts">
	import { page } from '$app/state';
	import Mark from '$lib/Mark.svelte';
	import type { Mark as MarkShape } from '$lib/api';
	import '../app.css';

	let { children } = $props();

	/**
	 * Three tabs, drawn as the three basic shapes: circle is time, square is a
	 * project, triangle is a plan. The log is a segment of the schedule, which is
	 * what keeps the bar down to three thumb-sized targets.
	 *
	 * Settings is here rather than in the bar: on a phone it hangs off the
	 * timer's header, and on a desktop there is room for it at the foot of the
	 * sidebar. Either way it is one link, not a fourth tab.
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
		<div class="marks">
			{#each tabs as tab (tab.href)}
				{@const current = isCurrent(tab.href)}
				<a href={tab.href} aria-current={current ? 'page' : undefined}>
					<Mark mark={tab.mark} size={22} color={current ? 'var(--paper)' : 'var(--ink)'} />
					<span class="tab-label">{tab.label}</span>
				</a>
			{/each}
		</div>

		<a
			class="settings"
			href="/settings"
			aria-current={page.url.pathname === '/settings' ? 'page' : undefined}
		>
			Settings
		</a>
	</nav>
</div>

<style>
	/*
	 * One grid, two arrangements. On a phone it is content over a bar; from 900px
	 * the same two children become a sidebar beside content, so nothing has to be
	 * rendered twice or moved in the markup.
	 */
	.shell {
		display: grid;
		/* `minmax(0, 1fr)`, not `1fr`: a grid track refuses to shrink below its
		   content's minimum width, so one unbreakable row would push the whole
		   screen wider than the phone it is on. */
		grid-template-columns: minmax(0, 1fr);
		grid-template-rows: 1fr auto;
		height: 100dvh;
		max-width: var(--shell);
		margin: 0 auto;
		background: var(--paper);
	}

	main {
		min-width: 0;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		padding-top: env(safe-area-inset-top);
	}

	nav {
		border-top: var(--rule) solid var(--ink);
		padding-bottom: env(safe-area-inset-bottom);
	}

	.marks {
		display: flex;
	}

	.marks a {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 58px;
		text-decoration: none;
	}

	.marks a + a {
		border-left: var(--rule) solid var(--ink);
	}

	nav a[aria-current='page'] {
		background: var(--ink);
		color: var(--paper);
	}

	/* Present for screen readers at every width; only drawn when there is a
	   sidebar to draw it in. */
	.tab-label,
	.settings {
		position: absolute;
		width: 1px;
		height: 1px;
		margin: -1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	@media (min-width: 700px) {
		.shell {
			max-width: none;
			grid-template-rows: none;
			grid-template-columns: var(--sidebar) minmax(0, 1fr);
		}

		/* The sidebar is first in the grid but second in the markup, so the tab
		   bar keeps its reading order on a phone: content, then navigation. */
		nav {
			grid-column: 1;
			grid-row: 1;
			display: flex;
			flex-direction: column;
			border-top: none;
			border-right: var(--rule) solid var(--ink);
			/* No bottom padding: the settings link sits flush to the foot, so a
			   selected one fills the corner instead of floating above a strip.
			   The bar's inset has to be cleared for that, not just left unset.
			   The left inset replaces it: a notched phone in landscape is past
			   700px, and the marks would otherwise sit under the housing. */
			padding-top: var(--pad);
			padding-bottom: 0;
			padding-left: env(safe-area-inset-left);
		}

		main {
			grid-column: 2;
			grid-row: 1;
			/* Centres the content in a readable measure without the screens having
			   to know they are on a desktop. The sidebar absorbs the left inset,
			   so only the right one has to be kept clear here — and only when the
			   centring gutter is not already wider than it. */
			padding-inline: max(0px, calc((100% - var(--measure)) / 2))
				max(env(safe-area-inset-right), calc((100% - var(--measure)) / 2));
		}

		.marks {
			flex-direction: column;
		}

		.marks a {
			justify-content: flex-start;
			gap: 14px;
			min-height: 52px;
			padding: 0 var(--pad);
		}

		.marks a + a {
			border-left: none;
		}

		.tab-label,
		.settings {
			position: static;
			width: auto;
			height: auto;
			margin: 0;
			overflow: visible;
			clip-path: none;
			font-size: 0.8125rem;
			font-weight: 500;
			letter-spacing: 0.14em;
			text-transform: uppercase;
		}

		.settings {
			margin-top: auto;
			padding: 14px var(--pad);
			text-decoration: none;
			color: var(--ink-60);
		}
	}

	@media (hover: hover) {
		nav a:not([aria-current='page']):hover {
			background: var(--paper-sunk);
		}
	}
</style>
