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

const NTFY = {
	server: 'https://ntfy.sh',
	topic: null,
	appUrl: null,
	hasToken: false,
	subscribeUrl: null,
	test: null
};

const SETTINGS = {
	timezone: 'UTC',
	focus: '25m',
	shortBreak: '5m',
	longBreak: '15m',
	longBreakEvery: 4,
	remindBefore: '5m'
};

/** Routed by path: the screen makes two reads, and they want different shapes. */
function stubApi(ntfy: unknown = NTFY): void {
	vi.stubGlobal('fetch', (url: string) =>
		Promise.resolve(
			new Response(JSON.stringify(url.startsWith('/api/ntfy') ? ntfy : SETTINGS), {
				headers: { 'content-type': 'application/json' }
			})
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

describe('the ntfy panel', () => {
	/**
	 * "Delivered" on its own reads as a guarantee the test send cannot make:
	 * ntfy answers 200 for any topic name, so a typo looks exactly like success.
	 */
	it('says a delivered test still does not prove the topic', async () => {
		stubApi({
			...NTFY,
			topic: 'timemd-a7f3',
			subscribeUrl: 'https://ntfy.sh/timemd-a7f3',
			test: 'delivered'
		});
		render(Settings);

		await vi.waitFor(() => {
			expect(screen.getByRole('status')).toHaveTextContent(/ntfy accepts any name/i);
		});
	});

	it('offers the subscribe URL once a topic is set', async () => {
		stubApi({ ...NTFY, topic: 'timemd-a7f3', subscribeUrl: 'https://ntfy.sh/timemd-a7f3' });
		render(Settings);

		await vi.waitFor(() => {
			expect(screen.getByText('https://ntfy.sh/timemd-a7f3')).toBeInTheDocument();
		});
		expect(screen.getByRole('button', { name: /turn off ntfy/i })).toBeInTheDocument();
	});

	/** The token has no server value to hold, so the box must not look like it has one. */
	it('never prefills the token box', async () => {
		stubApi({ ...NTFY, topic: 'timemd-a7f3', hasToken: true });
		render(Settings);

		const token = await vi.waitFor(() => screen.getByPlaceholderText(/type to replace/i));
		expect(token).toHaveValue('');
	});

	it('shows empty instructional placeholders when nothing is configured', async () => {
		stubApi();
		render(Settings);

		const topic = await vi.waitFor(() => screen.getByLabelText('Topic'));
		expect(topic).toHaveValue('');
		expect(topic).toHaveAttribute('placeholder', 'a name nobody would guess');
		expect(screen.getByLabelText('Server')).toHaveValue('');
		expect(screen.getByLabelText('App URL')).toHaveValue('');
		expect(document.body.innerHTML).not.toContain('timemd-a7f3c9e1');
		expect(document.body.innerHTML).not.toContain('box.tailnet.ts.net');
	});

	it('does not submit an example topic or app URL on save', async () => {
		const writes: unknown[] = [];
		vi.stubGlobal('fetch', (url: string, init?: RequestInit) => {
			if (init?.method === 'PUT' && url.startsWith('/api/ntfy')) {
				const body = JSON.parse(String(init.body));
				writes.push(body);
				return Promise.resolve(
					new Response(JSON.stringify({ ...NTFY, ...body }), {
						headers: { 'content-type': 'application/json' }
					})
				);
			}
			return Promise.resolve(
				new Response(JSON.stringify(url.startsWith('/api/ntfy') ? NTFY : SETTINGS), {
					headers: { 'content-type': 'application/json' }
				})
			);
		});
		render(Settings);

		const save = await vi.waitFor(() => {
			const button = screen.getByRole('button', { name: /^save$/i });
			expect(button).not.toBeDisabled();
			return button;
		});
		save.click();

		await vi.waitFor(() => expect(writes).toHaveLength(1));
		expect(writes[0]).toEqual({ server: '', topic: null, appUrl: null });
		expect(JSON.stringify(writes[0])).not.toMatch(/timemd-a7f3c9e1|box\.tailnet/);
	});
});

describe('the duration steppers', () => {
	it('says they move by five minutes', async () => {
		stubApi();
		render(Settings);

		await vi.waitFor(() => screen.getByRole('button', { name: /shorten focus by 5 minutes/i }));
		expect(screen.getByRole('button', { name: /lengthen break by 5 minutes/i })).toBeInTheDocument();
		expect(screen.getByText(/minutes, in steps of 5/i)).toBeInTheDocument();
	});
});
