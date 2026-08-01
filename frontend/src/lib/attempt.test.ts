import { describe as group, expect, it } from 'vitest';
import { ApiError } from './api';
import { attempt, describe } from './attempt';

group('attempt', () => {
	it('returns null when the work succeeds', async () => {
		await expect(attempt(() => Promise.resolve('fine'))).resolves.toBeNull();
	});

	it('returns the server message when the work throws an ApiError', async () => {
		const failed = attempt(() => Promise.reject(new ApiError(409, 'already exists')));
		await expect(failed).resolves.toBe('already exists');
	});

	it('falls back for anything that is not an ApiError', async () => {
		await expect(attempt(() => Promise.reject(new TypeError('boom')))).resolves.toBe(
			'Something went wrong'
		);
	});
});

group('describe', () => {
	it('prefers the server message', () => {
		expect(describe(new ApiError(400, 'bad range'))).toBe('bad range');
		expect(describe('a bare string')).toBe('Something went wrong');
		expect(describe(null)).toBe('Something went wrong');
	});
});
