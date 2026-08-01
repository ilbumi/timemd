import { describe, expect, it } from 'vitest';
import type { Project } from './api';
import { lookOf, looksFrom } from './look';
import { paletteColor } from './palette';

function project(overrides: Partial<Project> = {}): Project {
	return {
		slug: 'thesis',
		name: 'Thesis',
		color: '#245a8d',
		mark: 'square',
		target: '10h',
		status: 'active',
		created: '2026-08-01',
		milestones: [],
		problems: [],
		...overrides
	};
}

describe('looksFrom', () => {
	it('indexes by slug', () => {
		const looks = looksFrom([project(), project({ slug: 'russian', name: 'Russian' })]);
		expect(Object.keys(looks)).toEqual(['thesis', 'russian']);
		expect(looks.thesis).toEqual({ name: 'Thesis', color: '#245a8d', mark: 'square' });
	});
});

describe('lookOf', () => {
	it('reads a known project back', () => {
		expect(lookOf(looksFrom([project()]), 'thesis').name).toBe('Thesis');
	});

	it('draws time tracked against no project in ink', () => {
		const untagged = lookOf({}, null);
		expect(untagged.name).toBe('No project');
		expect(untagged.mark).toBe('bar');
		expect(lookOf({}, '')).toEqual(untagged);
	});

	/** An agent can log against a slug before the project file exists. */
	it('invents a stable look for a slug with no project file', () => {
		const invented = lookOf({}, 'not-created-yet');
		expect(invented.name).toBe('not-created-yet');
		expect(invented.color).toBe(paletteColor('not-created-yet', null));
	});
});
