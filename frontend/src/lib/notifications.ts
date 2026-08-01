/**
 * Turning on Web Push.
 *
 * The awkward part of this is platform, not code: iOS delivers push only to a
 * PWA that has been added to the Home Screen, so a phone that has not done that
 * will accept the permission prompt and then silently never notify. That case
 * gets its own outcome rather than being reported as success.
 */

import { api } from './api';

export type PushOutcome =
	| 'enabled'
	| 'denied'
	/** The browser has no push support at all. */
	| 'unsupported'
	/** iOS Safari, not yet installed to the Home Screen. */
	| 'needs-install'
	| 'failed';

/** Running as an installed app rather than a browser tab. */
export function isStandalone(): boolean {
	// `standalone` is a non-standard Safari property, hence the widened view of
	// navigator rather than a cast that discards the type entirely.
	const legacy = navigator as Navigator & { standalone?: boolean };
	return window.matchMedia('(display-mode: standalone)').matches || legacy.standalone === true;
}

export function isIos(): boolean {
	return /iPad|iPhone|iPod/.test(navigator.userAgent);
}

/** Whether this browser could do push at all, installed or not. */
export function isSupported(): boolean {
	return 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window;
}

/**
 * The VAPID public key as the bytes `pushManager.subscribe` wants.
 *
 * The server sends base64url; `atob` speaks base64. The padding and the two
 * substituted characters are the whole of the difference.
 */
export function urlBase64ToUint8Array(base64Url: string): Uint8Array<ArrayBuffer> {
	const padding = '='.repeat((4 - (base64Url.length % 4)) % 4);
	const base64 = (base64Url + padding).replace(/-/g, '+').replace(/_/g, '/');
	const raw = atob(base64);

	// Allocated over an explicit ArrayBuffer: `applicationServerKey` will not
	// accept a view that might be backed by a SharedArrayBuffer.
	const bytes = new Uint8Array(new ArrayBuffer(raw.length));
	for (let index = 0; index < raw.length; index += 1) {
		bytes[index] = raw.charCodeAt(index);
	}
	return bytes;
}

/** Registers the service worker, asks permission, and subscribes. */
export async function enablePush(): Promise<PushOutcome> {
	if (!isSupported()) {
		return isIos() && !isStandalone() ? 'needs-install' : 'unsupported';
	}

	const permission = await Notification.requestPermission();
	if (permission !== 'granted') {
		return 'denied';
	}

	try {
		const registration = await navigator.serviceWorker.register('/sw.js', { scope: '/' });
		await navigator.serviceWorker.ready;

		const { publicKey } = await api.pushKey();
		const subscription = await registration.pushManager.subscribe({
			userVisibleOnly: true,
			applicationServerKey: urlBase64ToUint8Array(publicKey)
		});

		const json = subscription.toJSON();
		if (!json.endpoint || !json.keys?.p256dh || !json.keys.auth) {
			return 'failed';
		}

		await api.subscribePush({
			endpoint: json.endpoint,
			p256dh: json.keys.p256dh,
			auth: json.keys.auth
		});
		return 'enabled';
	} catch {
		return 'failed';
	}
}

/** Whether this device already has a subscription registered. */
export async function isSubscribed(): Promise<boolean> {
	if (!isSupported()) return false;
	const registration = await navigator.serviceWorker.getRegistration('/');
	if (!registration) return false;
	return (await registration.pushManager.getSubscription()) !== null;
}
