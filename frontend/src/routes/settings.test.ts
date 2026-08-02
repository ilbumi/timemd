/**
 * The notifications toggle, which is the one control on this screen that can
 * fail in a way the user cannot retry out of.
 *
 * Its own file rather than a case in `screens.test.ts`: `vi.mock` is
 * file-scoped, and stubbing `$lib/notifications` there would reach every other
 * screen's render. It is also the only honest way to drive this — `togglePush`
 * lives in the route, and `make e2e` cannot hold a push subscription in
 * headless Chromium.
 */

import { render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiError } from '$lib/api';

import Settings from './settings/+page.svelte';

const disablePush = vi.fn(() => Promise.reject(new ApiError(500, 'push is down')));

vi.mock('$app/state', () => ({
	get page() {
		return { url: new URL('http://localhost/settings'), params: {} };
	}
}));

vi.mock('$lib/notifications', () => ({
	isSupported: () => true,
	isIos: () => false,
	isStandalone: () => true,
	isSubscribed: () => Promise.resolve(true),
	enablePush: () => Promise.resolve('enabled'),
	disablePush: () => disablePush()
}));

function stubApi(): void {
	vi.stubGlobal('fetch', () =>
		Promise.resolve(
			new Response(
				JSON.stringify({
					timezone: 'UTC',
					focus: '25m',
					shortBreak: '5m',
					longBreak: '15m',
					longBreakEvery: 4,
					remindBefore: '5m'
				}),
				{ headers: { 'content-type': 'application/json' } }
			)
		)
	);
}

afterEach(() => {
	vi.unstubAllGlobals();
	disablePush.mockClear();
});

describe('the notifications toggle', () => {
	/// A throw that skipped `busy = false` left the button disabled for good,
	/// with nothing on screen to say why — the one state a user cannot retry
	/// out of.
	it('says what went wrong and stays clickable when turning them off fails', async () => {
		stubApi();
		render(Settings);

		const button = await vi.waitFor(() =>
			screen.getByRole('button', { name: /turn off notifications/i })
		);
		button.click();

		await vi.waitFor(() => {
			expect(screen.getByRole('alert')).toHaveTextContent('push is down');
		});
		expect(button).not.toBeDisabled();
		expect(disablePush).toHaveBeenCalledOnce();
	});
});
