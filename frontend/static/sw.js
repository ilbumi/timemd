/**
 * Service worker: the only part of the app that runs when the app does not.
 *
 * Deliberately tiny — it shows what the server sent and focuses the app when
 * tapped. Anything cleverer here is code that ships independently of the page
 * and is awkward to update, so the page keeps the logic.
 */

self.addEventListener('install', () => {
	// Take over immediately rather than waiting for every tab to close; on a
	// phone that tab may never close.
	self.skipWaiting();
});

self.addEventListener('activate', (event) => {
	event.waitUntil(self.clients.claim());
});

self.addEventListener('push', (event) => {
	let payload = { title: 'timemd', body: '', url: '/' };
	try {
		payload = { ...payload, ...event.data.json() };
	} catch {
		// A push with no usable payload still deserves to be shown: a silent
		// push is worse than a vague one, and some user agents require that
		// every push results in a notification.
	}

	event.waitUntil(
		self.registration.showNotification(payload.title, {
			body: payload.body,
			icon: '/icon-192.png',
			badge: '/icon-192.png',
			// Collapse repeats of the same thing rather than stacking them.
			tag: payload.url,
			renotify: true,
			data: { url: payload.url }
		})
	);
});

self.addEventListener('notificationclick', (event) => {
	event.notification.close();
	const target = event.notification.data?.url ?? '/';

	event.waitUntil(
		self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clients) => {
			// Reuse an open window if there is one; opening a second copy of a
			// single-page app is disorienting.
			for (const client of clients) {
				if ('focus' in client) {
					client.navigate(target);
					return client.focus();
				}
			}
			return self.clients.openWindow(target);
		})
	);
});
