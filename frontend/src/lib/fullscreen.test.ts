/**
 * jsdom implements none of the Fullscreen API, which is exactly the platform
 * this module has to keep working on — an iPhone has none of it either. So the
 * unsupported case is the default here, and support is stubbed in where the
 * browser half is what is under test.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

import { Fullscreen } from './fullscreen.svelte';

/** Pretends the browser has the API, with a fullscreen element or without. */
function withApi(element: Element | null): { request: ReturnType<typeof vi.fn> } {
	const request = vi.fn().mockResolvedValue(undefined);
	Object.defineProperty(document, 'fullscreenEnabled', { value: true, configurable: true });
	Object.defineProperty(document, 'fullscreenElement', { value: element, configurable: true });
	document.documentElement.requestFullscreen = request;
	document.exitFullscreen = vi.fn().mockResolvedValue(undefined);
	return { request };
}

afterEach(() => {
	Reflect.deleteProperty(document, 'fullscreenEnabled');
	Reflect.deleteProperty(document, 'fullscreenElement');
	Reflect.deleteProperty(document.documentElement, 'requestFullscreen');
	Reflect.deleteProperty(document, 'exitFullscreen');
	vi.restoreAllMocks();
});

describe('the mode', () => {
	it('hides the chrome on a browser with no Fullscreen API', async () => {
		const mode = new Fullscreen();
		await mode.enter();
		expect(mode.active).toBe(true);

		await mode.exit();
		expect(mode.active).toBe(false);
	});

	it('asks the browser to fill the screen as well', async () => {
		const { request } = withApi(null);
		const mode = new Fullscreen();

		await mode.toggle();
		expect(mode.active).toBe(true);
		expect(request).toHaveBeenCalledOnce();
	});

	it('stays on when the browser refuses the request', async () => {
		withApi(null);
		document.documentElement.requestFullscreen = vi.fn().mockRejectedValue(new Error('gesture'));
		const mode = new Fullscreen();

		await mode.enter();
		expect(mode.active).toBe(true);
	});

	it('leaves the browser alone when it is not in fullscreen', async () => {
		withApi(null);
		const exit = vi.fn();
		document.exitFullscreen = exit;
		const mode = new Fullscreen();
		await mode.enter();

		await mode.toggle();
		expect(mode.active).toBe(false);
		expect(exit).not.toHaveBeenCalled();
	});

	it('leaves browser fullscreen when it is in it', async () => {
		withApi(document.documentElement);
		const exit = vi.fn().mockResolvedValue(undefined);
		document.exitFullscreen = exit;
		const mode = new Fullscreen();
		mode.active = true;

		await mode.exit();
		expect(exit).toHaveBeenCalledOnce();
		expect(mode.active).toBe(false);
	});

	it('survives a browser that throws on the way out', async () => {
		withApi(document.documentElement);
		document.exitFullscreen = vi.fn().mockRejectedValue(new Error('nope'));
		const mode = new Fullscreen();
		mode.active = true;

		await mode.exit();
		expect(mode.active).toBe(false);
	});
});

describe('watch', () => {
	it('puts the chrome back when the browser leaves fullscreen', async () => {
		withApi(document.documentElement);
		const mode = new Fullscreen();
		const stop = mode.watch();
		await mode.enter();

		// Esc, F11 or the window manager: the element goes, then the event.
		Object.defineProperty(document, 'fullscreenElement', { value: null, configurable: true });
		document.dispatchEvent(new Event('fullscreenchange'));
		expect(mode.active).toBe(false);

		stop();
	});

	it('keeps the mode on while the browser is still in fullscreen', async () => {
		withApi(document.documentElement);
		const mode = new Fullscreen();
		const stop = mode.watch();
		await mode.enter();

		document.dispatchEvent(new Event('fullscreenchange'));
		expect(mode.active).toBe(true);

		stop();
	});

	it('stops listening once torn down', async () => {
		withApi(document.documentElement);
		const mode = new Fullscreen();
		mode.watch()();
		await mode.enter();

		Object.defineProperty(document, 'fullscreenElement', { value: null, configurable: true });
		document.dispatchEvent(new Event('fullscreenchange'));
		expect(mode.active).toBe(true);
	});
});
