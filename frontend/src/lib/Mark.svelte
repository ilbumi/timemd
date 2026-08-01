<script lang="ts">
	/**
	 * A project's geometric mark.
	 *
	 * Every shape is one SVG path in a 20×20 box, so filled and outline are the
	 * same code path — which matters because archived projects are drawn outline
	 * only, and a CSS-border square could not become an outline triangle.
	 */
	import type { Mark } from '$lib/api';

	/** The circle is drawn as a `<circle>` below, so it is not a path. */
	const PATHS: Record<Exclude<Mark, 'circle'>, string> = {
		square: 'M1 1H19V19H1Z',
		triangle: 'M10 1L19 18H1Z',
		diamond: 'M10 1L19 10L10 19L1 10Z',
		bar: 'M1 7H19V13H1Z'
	};

	let {
		mark = 'square',
		color = 'currentColor',
		size = 20,
		outline = false,
		title = ''
	}: {
		mark?: Mark;
		color?: string;
		size?: number;
		outline?: boolean;
		title?: string;
	} = $props();
</script>

<svg
	width={size}
	height={size}
	viewBox="0 0 20 20"
	role={title === '' ? 'presentation' : 'img'}
	aria-label={title === '' ? undefined : title}
	aria-hidden={title === '' ? 'true' : undefined}
>
	{#if mark === 'circle'}
		<circle
			cx="10"
			cy="10"
			r={outline ? 8.5 : 9.5}
			fill={outline ? 'none' : color}
			stroke={outline ? color : 'none'}
			stroke-width="2"
		/>
	{:else}
		<path
			d={PATHS[mark]}
			fill={outline ? 'none' : color}
			stroke={outline ? color : 'none'}
			stroke-width="2"
			stroke-linejoin="round"
		/>
	{/if}
</svg>

<style>
	svg {
		display: block;
		flex: none;
	}
</style>
