import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	disablePush,
	enablePush,
	isIos,
	isStandalone,
	isSubscribed,
	isSupported,
	urlBase64ToUint8Array
} from './notifications';

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('urlBase64ToUint8Array', () => {
	it('decodes a padded base64url string', () => {
		// "hello" is aGVsbG8 in base64url, aGVsbG8= padded.
		expect(Array.from(urlBase64ToUint8Array('aGVsbG8'))).toEqual([104, 101, 108, 108, 111]);
	});

	/** The two characters that differ between base64 and base64url. */
	it('substitutes the url-safe alphabet', () => {
		const urlSafe = '-_8';
		const decoded = urlBase64ToUint8Array(urlSafe);
		expect(Array.from(decoded)).toEqual(Array.from(atob('+/8'), (c) => c.charCodeAt(0)));
	});

	it('handles a realistic 65-byte VAPID key', () => {
		const bytes = new Uint8Array(65).fill(4);
		const base64Url = btoa(String.fromCharCode(...bytes))
			.replace(/\+/g, '-')
			.replace(/\//g, '_')
			.replace(/=+$/, '');

		expect(urlBase64ToUint8Array(base64Url)).toHaveLength(65);
	});

	it('returns nothing for an empty key', () => {
		expect(urlBase64ToUint8Array('')).toHaveLength(0);
	});
});

describe('platform detection', () => {
	function withAgent(userAgent: string, standalone?: boolean): void {
		vi.stubGlobal('navigator', { userAgent, standalone });
	}

	it('spots iOS', () => {
		withAgent('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)');
		expect(isIos()).toBe(true);

		withAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)');
		expect(isIos()).toBe(false);
	});

	it('spots an installed app via the display mode', () => {
		withAgent('any');
		vi.stubGlobal('window', { matchMedia: () => ({ matches: true }) });
		expect(isStandalone()).toBe(true);
	});

	/** Safari does not report display-mode, so the legacy flag is the only signal. */
	it('falls back to Safari’s legacy standalone flag', () => {
		withAgent('iPhone', true);
		vi.stubGlobal('window', { matchMedia: () => ({ matches: false }) });
		expect(isStandalone()).toBe(true);
	});

	it('reports a browser tab as not installed', () => {
		withAgent('iPhone', false);
		vi.stubGlobal('window', { matchMedia: () => ({ matches: false }) });
		expect(isStandalone()).toBe(false);
	});

	it('reports push support only when every piece is present', () => {
		vi.stubGlobal('navigator', { serviceWorker: {}, userAgent: 'any' });
		vi.stubGlobal('window', { PushManager: class {}, Notification: class {} });
		expect(isSupported()).toBe(true);

		vi.stubGlobal('navigator', { userAgent: 'any' });
		expect(isSupported()).toBe(false);
	});
});

/** A browser that supports push, with the pieces the flow touches. */
function stubBrowser(options: {
	permission?: NotificationPermission;
	subscription?: unknown;
	registration?: unknown;
	userAgent?: string;
}): void {
	const registration = options.registration ?? {
		pushManager: {
			subscribe: () => Promise.resolve(options.subscription),
			getSubscription: () => Promise.resolve(options.subscription ?? null)
		}
	};

	vi.stubGlobal('navigator', {
		userAgent: options.userAgent ?? 'test',
		serviceWorker: {
			register: () => Promise.resolve(registration),
			getRegistration: () => Promise.resolve(registration),
			ready: Promise.resolve(registration)
		}
	});
	vi.stubGlobal('window', {
		PushManager: class {},
		Notification: class {},
		matchMedia: () => ({ matches: false })
	});
	vi.stubGlobal('Notification', {
		requestPermission: () => Promise.resolve(options.permission ?? 'granted')
	});
	vi.stubGlobal('fetch', (url: string) =>
		Promise.resolve(
			new Response(JSON.stringify(url.includes('/key') ? { publicKey: 'aGVsbG8' } : {}), {
				status: url.includes('/key') ? 200 : 201,
				headers: { 'content-type': 'application/json' }
			})
		)
	);
}

const validSubscription = {
	toJSON: () => ({
		endpoint: 'https://push.example/abc',
		keys: { p256dh: 'public', auth: 'secret' }
	})
};

describe('enablePush', () => {
	it('subscribes and reports success', async () => {
		stubBrowser({ subscription: validSubscription });
		await expect(enablePush()).resolves.toBe('enabled');
	});

	it('reports a refused permission', async () => {
		stubBrowser({ permission: 'denied', subscription: validSubscription });
		await expect(enablePush()).resolves.toBe('denied');
	});

	it('reports a browser with no push support', async () => {
		vi.stubGlobal('navigator', { userAgent: 'test' });
		vi.stubGlobal('window', {});
		await expect(enablePush()).resolves.toBe('unsupported');
	});

	/**
	 * The case that would otherwise look like success and then never deliver:
	 * iOS Safari outside an installed app.
	 */
	it('tells an uninstalled iPhone to add to the Home Screen', async () => {
		vi.stubGlobal('navigator', { userAgent: 'iPhone' });
		vi.stubGlobal('window', { matchMedia: () => ({ matches: false }) });
		await expect(enablePush()).resolves.toBe('needs-install');
	});

	it('reports a failure when subscribing throws', async () => {
		stubBrowser({
			registration: {
				pushManager: {
					subscribe: () => Promise.reject(new Error('no')),
					getSubscription: () => Promise.resolve(null)
				}
			}
		});
		await expect(enablePush()).resolves.toBe('failed');
	});

	it('reports a failure when the subscription is missing its keys', async () => {
		stubBrowser({ subscription: { toJSON: () => ({ endpoint: 'https://push.example/abc' }) } });
		await expect(enablePush()).resolves.toBe('failed');
	});
});

describe('disablePush', () => {
	/** Both halves: either one alone leaves a wrong state behind. */
	it('drops the subscription locally and on the server', async () => {
		const unsubscribe = vi.fn(() => Promise.resolve(true));
		const requests: string[] = [];
		stubBrowser({ subscription: { ...validSubscription, unsubscribe } });
		vi.stubGlobal('fetch', (url: string, init: RequestInit) => {
			requests.push(`${init.method} ${url}`);
			return Promise.resolve(new Response(null, { status: 204 }));
		});

		await expect(disablePush()).resolves.toBe(true);
		expect(requests).toEqual(['DELETE /api/push/subscribe']);
		expect(unsubscribe).toHaveBeenCalledOnce();
	});

	/// The server 404s an endpoint it has no record of, which is the state we
	/// were asking it to reach. Without this the button could never be turned
	/// off again.
	it('treats an endpoint the server has forgotten as already gone', async () => {
		const unsubscribe = vi.fn(() => Promise.resolve(true));
		stubBrowser({ subscription: { ...validSubscription, unsubscribe } });
		vi.stubGlobal('fetch', () =>
			Promise.resolve(
				new Response(JSON.stringify({ error: 'no such subscription' }), {
					status: 404,
					headers: { 'content-type': 'application/json' }
				})
			)
		);

		await expect(disablePush()).resolves.toBe(true);
		expect(unsubscribe).toHaveBeenCalledOnce();
	});

	/// Dropping the browser's half while the server keeps pushing into a live
	/// endpoint is the asymmetry this function exists to prevent, so anything
	/// that is not a 404 stops before the local unsubscribe.
	it('leaves any other failure to the caller', async () => {
		const unsubscribe = vi.fn(() => Promise.resolve(true));
		stubBrowser({ subscription: { ...validSubscription, unsubscribe } });
		vi.stubGlobal('fetch', () => Promise.resolve(new Response(null, { status: 500 })));

		await expect(disablePush()).rejects.toThrow();
		expect(unsubscribe).not.toHaveBeenCalled();
	});

	it('is a no-op with no subscription or on an unsupported browser', async () => {
		const calls: string[] = [];
		stubBrowser({});
		vi.stubGlobal('fetch', (url: string) => {
			calls.push(url);
			return Promise.resolve(new Response(null, { status: 204 }));
		});
		await expect(disablePush()).resolves.toBe(false);

		vi.stubGlobal('navigator', { userAgent: 'test' });
		vi.stubGlobal('window', {});
		await expect(disablePush()).resolves.toBe(false);

		expect(calls).toEqual([]);
	});
});

describe('isSubscribed', () => {
	it('is true once a subscription exists', async () => {
		stubBrowser({ subscription: validSubscription });
		await expect(isSubscribed()).resolves.toBe(true);
	});

	it('is false with no subscription and on an unsupported browser', async () => {
		stubBrowser({});
		await expect(isSubscribed()).resolves.toBe(false);

		vi.stubGlobal('navigator', { userAgent: 'test' });
		vi.stubGlobal('window', {});
		await expect(isSubscribed()).resolves.toBe(false);
	});
});
