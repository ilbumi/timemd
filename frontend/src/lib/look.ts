/**
 * How a project should be drawn, by slug.
 *
 * Every schedule view draws blocks and sessions that name a project by slug and
 * nothing else, so each of them needs the same lookup from slug to colour, mark
 * and display name — including the answer for time tracked against no project
 * at all, which the design shows in ink rather than hiding.
 */

import type { Mark, Project } from './api';
import { markFor, paletteColor } from './palette';

export interface Look {
	name: string;
	color: string;
	mark: Mark;
}

/** Untagged time. Ink and a bar, so it reads as "no project" rather than as a
    project whose colour happens to be grey. */
const UNTAGGED: Look = { name: 'No project', color: '#111111', mark: 'bar' };

export function looksFrom(projects: Project[]): Record<string, Look> {
	return Object.fromEntries(
		projects.map((project) => [
			project.slug,
			{
				name: project.name,
				color: paletteColor(project.slug, project.color),
				mark: markFor(project.mark)
			}
		])
	);
}

/**
 * The look for a slug, falling back to a derived one.
 *
 * A slug with no project file is not an error: an agent can log time against
 * `[[whatever]]` before creating the project, and the schedule still has to draw
 * it. It gets the same stable colour it would have had.
 */
export function lookOf(looks: Record<string, Look>, slug: string | null): Look {
	if (slug === null || slug === '') return UNTAGGED;
	return looks[slug] ?? { name: slug, color: paletteColor(slug, null), mark: 'square' };
}
