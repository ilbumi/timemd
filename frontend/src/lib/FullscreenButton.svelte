<script lang="ts">
	/**
	 * The one control for fullscreen mode, drawn as the four corners of a frame:
	 * pointing out to fill the screen, pointing in to come back.
	 *
	 * It lives in the running and break bars because the mode belongs to a
	 * session — the timer page leaves fullscreen the moment there is none, so
	 * this is never the only way back.
	 */
	import { fullscreen } from './fullscreen.svelte';

	/** Corners pushed to the edges of the 20×20 box, and pulled back into it. */
	const OUT = 'M1 7V1H7 M13 1H19V7 M19 13V19H13 M7 19H1V13';
	const IN = 'M7 1V7H1 M19 7H13V1 M13 19V13H19 M1 13H7V19';
</script>

<button
	class="expand"
	type="button"
	aria-pressed={fullscreen.active}
	aria-label={fullscreen.active ? 'Leave fullscreen' : 'Fullscreen'}
	onclick={() => void fullscreen.toggle()}
>
	<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">
		<path
			d={fullscreen.active ? IN : OUT}
			fill="none"
			stroke="currentColor"
			stroke-width="2"
			stroke-linecap="square"
		/>
	</svg>
</button>

<style>
	/*
	 * Drawn at the size of the glyph and given its reach by the overlay below,
	 * the same bargain the pattern editor's switch makes: a 44px box in the bar
	 * would set the height of a header that is otherwise one line of small caps.
	 */
	.expand {
		position: relative;
		flex: none;
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		min-height: 0;
		padding: 0;
		border: none;
		background: none;
		color: var(--ink);
	}

	/* 20px of glyph plus 12 on every side is the 44 a thumb needs. */
	.expand::after {
		content: '';
		position: absolute;
		inset: -12px;
	}
</style>
