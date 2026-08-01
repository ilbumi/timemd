/**
 * Placing blocks on a time axis.
 *
 * The day timeline and the week raster draw the same picture at different
 * scales, and the subtle part — stretching the visible window so a block outside
 * the default hours is still on screen — is worth having in one place rather
 * than in two that are free to drift.
 */

import { minutesOfDay } from './dates';

export interface Span {
	from: number;
	to: number;
}

/** Anything with a wall-clock start and end. Both schedule views' blocks fit. */
interface Timed {
	start: string;
	end: string;
}

/** Minutes since midnight, right now, on the device's own clock. */
export function minutesNow(now: Date = new Date()): number {
	return now.getHours() * 60 + now.getMinutes();
}

/**
 * The window to draw, never narrower than the default hours and always wide
 * enough to hold every block.
 */
export function spanOf(blocks: Timed[], from: number, to: number): Span {
	const starts = blocks.map((block) => minutesOfDay(block.start));
	const ends = blocks.map((block) => minutesOfDay(block.end));
	const start = Math.min(from, ...starts);
	// An hour of height is the floor: a span of zero would divide by zero in
	// `offsetIn`, and one of a few minutes would magnify a rounding error.
	return { from: start, to: Math.max(to, start + 60, ...ends) };
}

/** The hours to label, every `step`th one inside the span. */
export function hourMarks(span: Span, step: number): number[] {
	const marks: number[] = [];
	for (let hour = Math.ceil(span.from / 60); hour * 60 <= span.to; hour += step) {
		marks.push(hour);
	}
	return marks;
}

/** Where a time sits in the span, as a percentage from the top. */
export function offsetIn(span: Span, minutes: number): number {
	return ((minutes - span.from) / (span.to - span.from)) * 100;
}

/** Top and height for a block, as percentages — what both views position with. */
export function placeIn(span: Span, block: Timed): { top: number; height: number } {
	const top = offsetIn(span, minutesOfDay(block.start));
	return { top, height: offsetIn(span, minutesOfDay(block.end)) - top };
}
