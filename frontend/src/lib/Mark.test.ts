import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Mark from './Mark.svelte';
import type { Mark as MarkShape } from './api';

function shapeOf(container: HTMLElement): SVGElement | null {
	return container.querySelector('path, circle');
}

describe('Mark', () => {
	it('draws each shape', () => {
		for (const mark of ['square', 'triangle', 'diamond', 'bar'] as MarkShape[]) {
			const { container } = render(Mark, { mark });
			expect(container.querySelector('path')).not.toBeNull();
		}
	});

	/** The circle is the one shape a path would only approximate. */
	it('draws a circle as a circle element', () => {
		const { container } = render(Mark, { mark: 'circle' });
		expect(container.querySelector('circle')).not.toBeNull();
		expect(container.querySelector('path')).toBeNull();
	});

	it('fills by default and strokes when outlined', () => {
		const filled = render(Mark, { mark: 'square', color: '#245a8d' });
		expect(shapeOf(filled.container)?.getAttribute('fill')).toBe('#245a8d');
		expect(shapeOf(filled.container)?.getAttribute('stroke')).toBe('none');

		const outlined = render(Mark, { mark: 'square', color: '#245a8d', outline: true });
		expect(shapeOf(outlined.container)?.getAttribute('fill')).toBe('none');
		expect(shapeOf(outlined.container)?.getAttribute('stroke')).toBe('#245a8d');
	});

	/**
	 * A mark always sits next to text that names the project, so it is decoration
	 * and announcing it would say everything twice.
	 */
	it('is hidden from assistive tech', () => {
		const { container } = render(Mark, { mark: 'square' });
		expect(container.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
	});

	it('sizes the box without changing the geometry', () => {
		const { container } = render(Mark, { mark: 'square', size: 38 });
		const svg = container.querySelector('svg');
		expect(svg).toHaveAttribute('width', '38');
		expect(svg).toHaveAttribute('viewBox', '0 0 20 20');
	});
});
