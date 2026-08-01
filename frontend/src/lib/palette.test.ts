import { describe, expect, it } from 'vitest';
import { IDENTITIES, PALETTE, contrastInk, markFor, paletteColor } from './palette';

describe('IDENTITIES', () => {
	it('offers every mark exactly once, each with its own colour', () => {
		expect(IDENTITIES.map((identity) => identity.mark)).toEqual([
			'square',
			'circle',
			'triangle',
			'diamond',
			'bar'
		]);
		expect(new Set(IDENTITIES.map((identity) => identity.color)).size).toBe(IDENTITIES.length);
	});
});

describe('paletteColor', () => {
	it('prefers the colour the project file gives', () => {
		expect(paletteColor('thesis', '#123456')).toBe('#123456');
	});

	it('falls back to a palette entry when there is none', () => {
		expect(PALETTE).toContain(paletteColor('thesis', null));
	});

	/** The fallback has to be stable, or a project changes colour on reload. */
	it('gives the same slug the same colour every time', () => {
		expect(paletteColor('thesis', null)).toBe(paletteColor('thesis', null));
		expect(paletteColor('russian', null)).toBe(paletteColor('russian', null));
	});

	it('spreads the first few slugs across different entries', () => {
		const chosen = new Set(['a', 'b', 'c', 'd', 'e'].map((slug) => paletteColor(slug, null)));
		expect(chosen.size).toBeGreaterThan(1);
	});
});

describe('contrastInk', () => {
	it('puts paper on the dark primaries and ink on the light ones', () => {
		expect(contrastInk('#245a8d')).toBe('#f2efe6');
		expect(contrastInk('#d1332e')).toBe('#f2efe6');
		expect(contrastInk('#8b6f8e')).toBe('#f2efe6');
		expect(contrastInk('#e9b83a')).toBe('#111111');
		expect(contrastInk('#ffffff')).toBe('#111111');
	});

	it('treats an unreadable colour as dark, which is the common case', () => {
		expect(contrastInk('not a colour')).toBe('#f2efe6');
	});
});

describe('markFor', () => {
	it('passes through the five marks the file format allows', () => {
		for (const mark of ['square', 'circle', 'triangle', 'diamond', 'bar'] as const) {
			expect(markFor(mark)).toBe(mark);
		}
	});

	/** Reads are lenient on the server; the client should not be stricter. */
	it('falls back to a square for anything else', () => {
		expect(markFor(null)).toBe('square');
		expect(markFor('hexagon')).toBe('square');
	});
});
