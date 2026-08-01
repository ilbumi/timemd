import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import ScheduleTabs from './ScheduleTabs.svelte';

const page = { url: new URL('http://localhost/schedule') };
vi.mock('$app/state', () => ({
	get page() {
		return page;
	}
}));

describe('ScheduleTabs', () => {
	it('links to the three views', () => {
		render(ScheduleTabs);
		expect(screen.getByRole('link', { name: 'Day' })).toHaveAttribute('href', '/schedule');
		expect(screen.getByRole('link', { name: 'Week' })).toHaveAttribute('href', '/schedule/week');
		expect(screen.getByRole('link', { name: 'Log' })).toHaveAttribute('href', '/schedule/log');
	});

	it('marks the current view', () => {
		render(ScheduleTabs);
		expect(screen.getByRole('link', { name: 'Day' })).toHaveAttribute('aria-current', 'page');
		expect(screen.getByRole('link', { name: 'Week' })).not.toHaveAttribute('aria-current');
	});

	/** `/schedule` is a prefix of the other two, so an exact match is what keeps
	    DAY from staying lit on the week and log views. */
	it('matches the path exactly rather than by prefix', () => {
		page.url = new URL('http://localhost/schedule/week');
		render(ScheduleTabs);

		expect(screen.getByRole('link', { name: 'Week' })).toHaveAttribute('aria-current', 'page');
		expect(screen.getByRole('link', { name: 'Day' })).not.toHaveAttribute('aria-current');
	});
});
