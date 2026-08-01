import { sveltekit } from '@sveltejs/kit/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit(), svelteTesting()],
	server: {
		// `vite dev` serves the UI; the API still comes from the Rust binary.
		proxy: {
			'/api': 'http://127.0.0.1:8080'
		}
	},
	test: {
		environment: 'jsdom',
		globals: true,
		setupFiles: ['./src/tests/setup.ts'],
		include: ['src/**/*.test.ts'],
		coverage: {
			provider: 'v8',
			include: ['src/lib/**'],
			thresholds: {
				lines: 85,
				functions: 85,
				branches: 80,
				statements: 85
			}
		}
	}
});
