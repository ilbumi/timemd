/**
 * The timer's other three screens.
 *
 * The home route is five screens in one file — welcome, running, break,
 * complete and the idle picker — and only the last of those is what the app
 * serves from a fixture at rest. The other four carry the most drawn furniture
 * in the app (a dial, an inverted field, a red band over a form) and none of it
 * was ever measured.
 *
 * They are reached by stubbing `GET /api/timer` rather than by driving the real
 * timer: the state lives in one file the server rewrites, so racing the other
 * specs for it would make every one of them flaky. A route handler is per-page
 * and per-test, so these stay parallel-safe.
 */
import { expect, test } from '@playwright/test';
import { WIDTHS, expectWellAligned } from './probes';

const NOW = '2026-08-01T09:10:00';

/** The shape `readTimer` returns; only `active` differs between states. */
function timerState(active: Record<string, unknown> | null, completedToday: number) {
	return {
		active,
		completedToday,
		trackedToday: '1h35m',
		nextBreak: '5m',
		nextBreakKind: 'short_break',
		serverNow: NOW
	};
}

function running(kind: 'focus' | 'short_break', remainingSeconds = 900) {
	return {
		kind,
		project: kind === 'focus' ? 'thesis' : null,
		note: kind === 'focus' ? 'chapter four, first pass' : '',
		startedAt: '2026-08-01T09:00:00',
		endsAt: '2026-08-01T09:25:00',
		duration: '25m',
		durationSeconds: 1500,
		remainingSeconds
	};
}

/** Serves one timer state for every poll until the test changes it. */
async function stubTimer(
	page: import('@playwright/test').Page,
	next: () => ReturnType<typeof timerState>
): Promise<void> {
	await page.route('**/api/timer', async (route) => {
		if (route.request().method() !== 'GET') return route.fallback();
		await route.fulfill({ json: next() });
	});
}

for (const width of [WIDTHS.phone, WIDTHS.sidebar, WIDTHS.desktop]) {
	test(`running focus @ ${width}px`, async ({ page }) => {
		await page.setViewportSize({ width, height: 900 });
		await stubTimer(page, () => timerState(running('focus'), 3));
		await page.goto('/');
		await page.locator('.dial').waitFor({ state: 'visible' });
		await expectWellAligned(page, width);
	});

	test(`break @ ${width}px`, async ({ page }) => {
		await page.setViewportSize({ width, height: 900 });
		await stubTimer(page, () => timerState(running('short_break'), 3));
		await page.goto('/');
		await page.locator('.ring').waitFor({ state: 'visible' });
		await expectWellAligned(page, width);
	});

	test(`session complete @ ${width}px`, async ({ page }) => {
		await page.setViewportSize({ width, height: 900 });

		// The completion screen is not a state the server reports — it is the
		// transition into one. The client shows it when a focus block that *was*
		// running has gone, and `completedToday` has grown: read that way so the
		// server's own tick counts, not only a stop the tab asked for.
		//
		// The block has a second left, so the countdown runs out and the screen
		// re-asks the server by itself. Waiting for the 20s poll instead made
		// this the slowest test in the suite by a factor of ten.
		let completed = false;
		await stubTimer(page, () =>
			completed ? timerState(null, 4) : timerState(running('focus', 1), 3)
		);
		await page.goto('/');
		await page.locator('.dial').waitFor({ state: 'visible' });

		completed = true;
		await page.locator('.complete').waitFor({ state: 'visible' });
		await expectWellAligned(page, width);
	});
}

/**
 * Fullscreen mode, measured the same way as the screens it hides the chrome on.
 *
 * The browser's own fullscreen is a side effect of the click; what is under
 * test is the app's half — the navigation gone, and the timer still square with
 * its edges once it is the only thing on the screen.
 */
for (const width of [WIDTHS.phone, WIDTHS.desktop]) {
	test(`fullscreen @ ${width}px`, async ({ page }) => {
		await page.setViewportSize({ width, height: 900 });
		await stubTimer(page, () => timerState(running('focus'), 3));
		await page.goto('/');
		await page.locator('.dial').waitFor({ state: 'visible' });

		await page.getByRole('button', { name: 'Fullscreen', exact: true }).click();
		await expect(page.locator('nav')).toBeHidden();
		await expectWellAligned(page, width);

		await page.getByRole('button', { name: 'Leave fullscreen' }).click();
		await expect(page.locator('nav')).toBeVisible();
	});
}
