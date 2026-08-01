import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Report } from './api';
import { readTotals, totalsFor, totalsFrom } from './totals';

afterEach(() => {
	vi.unstubAllGlobals();
});

function report(buckets: Report['buckets']): Report {
	return { from: '2026-07-27', to: '2026-08-02', groupBy: 'project', total: '7h', buckets };
}

describe('totalsFrom', () => {
	it('keys the rows by slug and reads the durations as minutes', () => {
		const rows = totalsFrom(
			report([
				{ key: 'thesis', tracked: '6h20m', sessions: 14 },
				{ key: 'russian', tracked: '1h10m', sessions: 3 }
			])
		);

		expect(rows).toEqual({
			thesis: { tracked: 380, sessions: 14 },
			russian: { tracked: 70, sessions: 3 }
		});
	});

	/** A null key is untagged time, which belongs to no project's target. */
	it('drops the bucket for time tracked against no project', () => {
		const rows = totalsFrom(report([{ key: null, tracked: '45m', sessions: 2 }]));
		expect(rows).toEqual({});
	});
});

describe('totalsFor', () => {
	it('reads a row back', () => {
		expect(totalsFor({ thesis: { tracked: 380, sessions: 14 } }, 'thesis')).toEqual({
			tracked: 380,
			sessions: 14
		});
	});

	it('reports zero for a project with nothing tracked', () => {
		expect(totalsFor({}, 'thesis')).toEqual({ tracked: 0, sessions: 0 });
	});
});

describe('readTotals', () => {
	it('asks for the range grouped by project', async () => {
		let asked = '';
		vi.stubGlobal('fetch', (url: string) => {
			asked = url;
			return Promise.resolve(
				new Response(JSON.stringify(report([{ key: 'thesis', tracked: '2h', sessions: 4 }])), {
					headers: { 'content-type': 'application/json' }
				})
			);
		});

		const rows = await readTotals('2026-07-27', '2026-08-02');

		expect(asked).toBe('/api/reports?from=2026-07-27&to=2026-08-02&groupBy=project');
		expect(rows.thesis).toEqual({ tracked: 120, sessions: 4 });
	});

	/** A target bar is worth losing; a screen is not. */
	it('resolves to nothing when the request fails', async () => {
		vi.stubGlobal('fetch', () => Promise.reject(new Error('offline')));
		expect(await readTotals('2026-07-27', '2026-08-02')).toEqual({});
	});
});
