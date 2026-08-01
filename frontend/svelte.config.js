import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/**
 * Builds a single-page app straight into the Rust crate's asset directory, where
 * `rust-embed` picks it up at compile time. One binary ships the whole app.
 *
 * `fallback` makes it an SPA: routing happens client-side against a JSON API on
 * the same origin, so there is nothing to prerender.
 */
export default {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({
			pages: '../crates/server/assets',
			assets: '../crates/server/assets',
			fallback: 'index.html',
			precompress: false,
			strict: true
		})
	}
};
