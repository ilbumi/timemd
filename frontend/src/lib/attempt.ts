import { ApiError } from './api';

/**
 * Runs an API call and turns any failure into a message to show.
 *
 * Every screen needs this and each had written its own copy, which meant the
 * fallback wording and the error handling could drift per route.
 *
 * @returns the error message, or `null` when the call succeeded.
 */
export async function attempt(work: () => Promise<unknown>): Promise<string | null> {
	try {
		await work();
		return null;
	} catch (failure) {
		return describe(failure);
	}
}

/** What to show the user for a thrown value. */
export function describe(failure: unknown): string {
	return failure instanceof ApiError ? failure.message : 'Something went wrong';
}
