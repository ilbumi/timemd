/**
 * Fullscreen mode: the running session, and nothing else on the screen.
 *
 * Two things happen at once, and they are kept separate on purpose. The app
 * hides its own chrome — the tab bar, the sidebar — which works everywhere,
 * including an iPhone, where there is no Fullscreen API at all. Where the
 * browser does offer one it is asked for as well, so the browser's own
 * furniture goes with it.
 *
 * The browser's half is the one that changes behind our back: Esc, F11 and the
 * window manager all leave fullscreen without asking. So `fullscreenchange` is
 * listened to and leaving that way puts the chrome back too — the two halves
 * are never allowed to disagree, because a screen that is no longer fullscreen
 * but still hiding its navigation is a screen with no way off it.
 */

/** Whether this browser will let the page fill the screen. */
export function isSupported(): boolean {
	return typeof document !== 'undefined' && document.fullscreenEnabled === true;
}

export class Fullscreen {
	/** Whether the app is drawing the session alone. */
	active = $state(false);

	/** Follows the browser leaving fullscreen on its own. Returns the teardown. */
	watch(): () => void {
		const sync = (): void => {
			if (isSupported() && document.fullscreenElement === null) this.active = false;
		};
		document.addEventListener('fullscreenchange', sync);
		return () => document.removeEventListener('fullscreenchange', sync);
	}

	async enter(): Promise<void> {
		this.active = true;
		try {
			if (isSupported()) await document.documentElement.requestFullscreen();
		} catch {
			// Best effort. The chrome is already gone, and a browser that refuses —
			// a request outside a user gesture, or a policy that forbids it — must
			// not take the mode down with it.
		}
	}

	async exit(): Promise<void> {
		this.active = false;
		try {
			if (isSupported() && document.fullscreenElement !== null) await document.exitFullscreen();
		} catch {
			// Likewise: we are already out as far as the app is concerned.
		}
	}

	toggle(): Promise<void> {
		return this.active ? this.exit() : this.enter();
	}
}

/** One mode for the whole app: the layout hides chrome, a screen toggles it. */
export const fullscreen = new Fullscreen();
