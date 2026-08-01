/**
 * The layout suite. It exists because this design language is enforced by hand
 * across one global sheet and ten scoped `<style>` blocks, and the last five
 * commits on this branch were all alignment fixes — several of them the same
 * bug landing in one screen and not its three siblings.
 *
 * These are assertions, not screenshots: a snapshot needs a human to look at
 * it, and the properties the design actually promises (one edge per screen,
 * one rule where two meet, no radius anywhere) can be checked outright.
 *
 * Chromium only. This is one engine's worth of geometry; three browsers would
 * triple the run for no extra signal.
 */
import { defineConfig, devices } from '@playwright/test';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..');

/** Never `./data` — that is the developer's real time log, and the server writes to it. */
export const DATA_DIR = resolve(repoRoot, '.e2e-data');

export const BASE_URL = 'http://127.0.0.1:8080';

export default defineConfig({
	testDir: './e2e',
	globalSetup: './e2e/global-setup.ts',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: 0,
	reporter: process.env.CI ? 'list' : [['list'], ['html', { open: 'never' }]],
	use: {
		baseURL: BASE_URL,
		...devices['Desktop Chrome']
	},
	webServer: {
		// The UI is embedded in the binary at compile time, so `make e2e` builds
		// the frontend first; this only compiles and runs the server.
		command: `cargo run --quiet --bin timemd -- --data ${DATA_DIR} serve --addr 127.0.0.1:8080`,
		cwd: repoRoot,
		url: `${BASE_URL}/api/health`,
		reuseExistingServer: !process.env.CI,
		timeout: 240_000,
		stdout: 'ignore',
		stderr: 'pipe'
	}
});
