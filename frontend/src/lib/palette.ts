/**
 * Colour and shape for a project.
 *
 * The design identifies a project by its mark first and its colour second, so
 * both have to resolve to *something* for every project — including one an agent
 * created by dropping a file in `projects/` with no `color` and no `mark`.
 */

import type { Mark } from './api';

/** The workshop palette, in the order the mark picker offers it. */
export const PALETTE = ['#245a8d', '#d1332e', '#e9b83a', '#8b6f8e', '#4a6b63'] as const;

const MARKS: readonly Mark[] = ['square', 'circle', 'triangle', 'diamond', 'bar'];

/**
 * The five identities the picker offers, each pairing a shape with a colour.
 *
 * Offered as pairs rather than two independent choices because that is what
 * makes projects distinguishable: two shapes in similar colours, or two colours
 * in the same shape, are exactly the collisions the mark exists to avoid.
 */
export const IDENTITIES: readonly { mark: Mark; color: string }[] = MARKS.map((mark, index) => ({
	mark,
	color: PALETTE[index] ?? PALETTE[0]
}));

/** What a project gets before anyone chooses. Widened from the `as const` tuple
    so a `$state` seeded with it stays assignable from any colour. */
export const DEFAULT_COLOR: string = PALETTE[0];

const PAPER = '#f2efe6';
const INK = '#111111';

/**
 * The project's own colour, or a stable palette entry derived from its slug.
 *
 * Derived rather than random: a project that changed colour between two renders
 * would defeat the point of identifying it by colour at all.
 */
export function paletteColor(slug: string, color: string | null): string {
	if (color !== null && color !== '') return color;

	let hash = 0;
	for (const character of slug) {
		hash = (hash * 31 + (character.codePointAt(0) ?? 0)) % 100_000;
	}
	return PALETTE[hash % PALETTE.length] ?? PALETTE[0];
}

/** Paper or ink, whichever stays legible on the given fill. */
export function contrastInk(background: string): string {
	const hex = background.trim();
	// An unreadable value is far more likely to be a dark primary than a pale
	// one, and paper-on-dark is the design's default pairing.
	if (!/^#[0-9a-f]{6}$/i.test(hex)) return PAPER;

	const value = Number.parseInt(hex.slice(1), 16);
	// Rec. 601 luma, which is enough to separate this palette's five colours.
	const luma =
		0.299 * ((value >> 16) & 0xff) + 0.587 * ((value >> 8) & 0xff) + 0.114 * (value & 0xff);
	return luma > 150 ? INK : PAPER;
}

/** The stored mark, or a square when the file did not say. */
export function markFor(mark: string | null): Mark {
	return MARKS.find((candidate) => candidate === mark) ?? 'square';
}
