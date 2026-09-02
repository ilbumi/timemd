import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import IdentityPicker from './IdentityPicker.svelte';
import { MARKS, PALETTE } from './palette';

describe('IdentityPicker', () => {
	it('offers every mark and every colour as its own radio', () => {
		render(IdentityPicker, { mark: 'square', color: '#245a8d' });
		expect(
			screen.getByRole('radiogroup', { name: 'Mark' }).querySelectorAll('[role="radio"]')
		).toHaveLength(MARKS.length);
		expect(
			screen.getByRole('radiogroup', { name: 'Colour' }).querySelectorAll('[role="radio"]')
		).toHaveLength(PALETTE.length);
	});

	it('marks the current shape and colour as checked', () => {
		render(IdentityPicker, { mark: 'triangle', color: '#e9b83a' });
		expect(screen.getByRole('radio', { name: 'triangle' })).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByRole('radio', { name: 'square' })).toHaveAttribute('aria-checked', 'false');
		expect(screen.getByRole('radio', { name: 'yellow' })).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByRole('radio', { name: 'blue' })).toHaveAttribute('aria-checked', 'false');
	});

	it('changes the mark without forcing a colour', async () => {
		render(IdentityPicker, { mark: 'square', color: '#245a8d' });

		await userEvent.click(screen.getByRole('radio', { name: 'circle' }));

		expect(screen.getByRole('radio', { name: 'circle' })).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByRole('radio', { name: 'square' })).toHaveAttribute('aria-checked', 'false');
		expect(screen.getByRole('radio', { name: 'blue' })).toHaveAttribute('aria-checked', 'true');
	});

	it('changes the colour without forcing a mark', async () => {
		render(IdentityPicker, { mark: 'square', color: '#245a8d' });

		await userEvent.click(screen.getByRole('radio', { name: 'red' }));

		expect(screen.getByRole('radio', { name: 'red' })).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByRole('radio', { name: 'blue' })).toHaveAttribute('aria-checked', 'false');
		expect(screen.getByRole('radio', { name: 'square' })).toHaveAttribute('aria-checked', 'true');
	});
});
