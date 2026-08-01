import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import IdentityPicker from './IdentityPicker.svelte';
import { IDENTITIES } from './palette';

describe('IdentityPicker', () => {
	it('offers every identity as a radio', () => {
		render(IdentityPicker, { mark: 'square', color: '#245a8d' });
		expect(screen.getAllByRole('radio')).toHaveLength(IDENTITIES.length);
	});

	it('marks the current one as checked', () => {
		render(IdentityPicker, { mark: 'triangle', color: '#e9b83a' });
		expect(screen.getByRole('radio', { name: 'triangle' })).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByRole('radio', { name: 'square' })).toHaveAttribute('aria-checked', 'false');
	});

	it('moves the selection when another is picked', async () => {
		render(IdentityPicker, { mark: 'square', color: '#245a8d' });

		await userEvent.click(screen.getByRole('radio', { name: 'diamond' }));

		expect(screen.getByRole('radio', { name: 'diamond' })).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByRole('radio', { name: 'square' })).toHaveAttribute('aria-checked', 'false');
	});
});
