import { DATA_DIR } from '../playwright.config';
import { seed } from './seed';

/**
 * Runs before the server starts. Ordering is not actually load-bearing — the
 * store has no cache and every read hits disk — but seeding first means the
 * first screenshot is never of an empty app.
 */
export default function globalSetup(): void {
	seed(DATA_DIR);
}
