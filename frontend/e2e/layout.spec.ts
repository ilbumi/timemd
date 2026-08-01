/**
 * Alignment and adaptive layout, screen by screen and width by width.
 *
 * The probes live in `probes.ts`; this file decides what to point them at and
 * asserts the adaptive behaviour the two breakpoints promise.
 */
import { test, expect, type Page } from '@playwright/test';
import { ALL, ENDS, SLACK, WIDTHS, expectWellAligned, open } from './probes';

const SCREENS: { name: string; path: string; widths: readonly number[] }[] = [
	{ name: 'timer', path: '/', widths: ALL },
	{ name: 'schedule-day', path: '/schedule', widths: ALL },
	{ name: 'schedule-week', path: '/schedule/week', widths: ALL },
	{ name: 'project-detail', path: '/projects/thesis', widths: ALL },
	{ name: 'projects', path: '/projects', widths: ENDS },
	{ name: 'project-new', path: '/projects/new', widths: ENDS },
	{ name: 'schedule-log', path: '/schedule/log', widths: ENDS },
	{ name: 'schedule-pattern', path: '/schedule/pattern', widths: ENDS },
	{ name: 'settings', path: '/settings', widths: ENDS }
];

for (const screen of SCREENS) {
	for (const width of screen.widths) {
		test(`${screen.name} @ ${width}px`, async ({ page }) => {
			await open(page, screen.path, width);
			await expectWellAligned(page, width);
		});
	}
}

test.describe('adaptive behaviour', () => {
	test('the nav is a bottom bar below 700px and a sidebar above it', async ({ page }) => {
		await open(page, '/', WIDTHS.tween);
		const nav = page.locator('nav');
		const main = page.locator('main');
		let navBox = (await nav.boundingBox())!;
		let mainBox = (await main.boundingBox())!;
		expect(navBox.y, 'bar sits below the content').toBeGreaterThan(mainBox.y);
		// The labels are clipped rather than removed — they still have to be
		// announced — so their width is what says whether they are drawn.
		expect((await page.locator('.tab-label').first().boundingBox())!.width).toBeLessThanOrEqual(1);

		await open(page, '/', WIDTHS.sidebar);
		navBox = (await nav.boundingBox())!;
		mainBox = (await main.boundingBox())!;
		expect(navBox.x, 'sidebar sits beside the content').toBeLessThan(mainBox.x);
		expect(navBox.width).toBeCloseTo(216, 0);
		expect((await page.locator('.tab-label').first().boundingBox())!.width).toBeGreaterThan(1);
	});

	test('the shell keeps the phone column below 700px', async ({ page }) => {
		await open(page, '/', WIDTHS.tween);
		const shell = (await page.locator('.shell').boundingBox())!;
		expect(shell.width).toBeCloseTo(440, 0);
	});

	test('the content column never exceeds its measure', async ({ page }) => {
		for (const width of [WIDTHS.wide, WIDTHS.desktop]) {
			await open(page, '/projects', width);
			const content = await page.evaluate(() => {
				const main = document.querySelector('main')!;
				const style = getComputedStyle(main);
				return (
					main.getBoundingClientRect().width -
					parseFloat(style.paddingLeft) -
					parseFloat(style.paddingRight)
				);
			});
			expect(content, `content column at ${width}px`).toBeLessThanOrEqual(900 + SLACK);
		}
	});

	test('two columns of content appear only once there is room for them', async ({ page }) => {
		await open(page, '/', WIDTHS.sidebar);
		expect(await columnCount(page, '.grid'), 'tiles at 760px').toBe(2);
		await open(page, '/schedule', WIDTHS.sidebar);
		expect(await columnCount(page, '.canvas'), 'timeline at 760px').toBe(1);

		await open(page, '/', WIDTHS.wide);
		expect(await columnCount(page, '.grid'), 'tiles at 1120px').toBe(4);
		await open(page, '/schedule', WIDTHS.wide);
		expect(await columnCount(page, '.canvas'), 'timeline at 1120px').toBe(2);
	});

	/**
	 * The split is a question about the content column, not the window. The
	 * sidebar takes 216px off the top, so the column reaches its 900px measure
	 * at a 1116px window — and never reaches 1000px at any window size, which
	 * is what the old `@media (min-width: 1000px)` was asking for.
	 */
	test('the split is decided by the content column, not the window', async ({ page }) => {
		await open(page, '/schedule', 1100);
		expect(await columnCount(page, '.canvas'), '884px of content is not two columns').toBe(1);

		await open(page, '/schedule', 1120);
		expect(await columnCount(page, '.canvas'), '904px of content is').toBe(2);
	});

	test('the project header becomes a side panel when wide', async ({ page }) => {
		await open(page, '/projects/thesis', WIDTHS.sidebar);
		let head = (await page.locator('.head').boundingBox())!;
		let body = (await page.locator('.body').boundingBox())!;
		expect(head.y + head.height, 'banner sits above the lists').toBeLessThanOrEqual(body.y + SLACK);

		await open(page, '/projects/thesis', WIDTHS.wide);
		head = (await page.locator('.head').boundingBox())!;
		body = (await page.locator('.body').boundingBox())!;
		expect(head.x + head.width, 'panel sits beside the lists').toBeLessThanOrEqual(body.x + SLACK);
	});

	/**
	 * The regression guard for the container query: `container-type` implies
	 * `contain: layout`, which would make its element the containing block for
	 * this backdrop and quietly stop the modal covering the sidebar.
	 */
	test('the delete sheet still covers the whole viewport', async ({ page }) => {
		// Only an archived project offers deletion, so this is the archived one.
		await open(page, '/projects/masters-course', WIDTHS.desktop);
		await page.getByRole('button', { name: /delete permanently/i }).click();
		const backdrop = page.locator('.sheet-backdrop');
		await expect(backdrop).toBeVisible();
		const box = (await backdrop.boundingBox())!;
		const viewport = page.viewportSize()!;
		expect(box.x).toBeCloseTo(0, 0);
		expect(box.y).toBeCloseTo(0, 0);
		expect(box.width).toBeCloseTo(viewport.width, 0);
	});
});

async function columnCount(page: Page, selector: string): Promise<number> {
	return page.evaluate((sel) => {
		const el = document.querySelector(sel);
		if (!el) throw new Error(`no ${sel} on this screen`);
		return getComputedStyle(el).gridTemplateColumns.split(/\s+/).filter(Boolean).length;
	}, selector);
}
