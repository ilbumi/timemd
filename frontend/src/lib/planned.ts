import type { DayView } from './api';

/**
 * Maps a position in the merged `planned` list to its index among the one-off
 * blocks, which is what the delete endpoint addresses.
 *
 * The merged list interleaves repeating blocks with one-offs in start order, so
 * the two indexes diverge as soon as a repeat sorts earlier than a one-off.
 */
export function oneOffIndex(day: DayView, plannedIndex: number): number {
	return day.planned.slice(0, plannedIndex).filter((block) => block.block === null).length;
}
