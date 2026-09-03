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
	{ name: 'todos', path: '/todos', widths: ALL },
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
		const phoneLabel = (await page.locator('.tab-label').first().boundingBox())!;
		const phoneMark = (await page.locator('.marks a').first().locator('svg').boundingBox())!;
		expect(phoneLabel.width, 'phone labels are drawn').toBeGreaterThan(1);
		expect(phoneLabel.y, 'phone labels sit under the mark').toBeGreaterThan(
			phoneMark.y + phoneMark.height - SLACK
		);

		await open(page, '/', WIDTHS.sidebar);
		navBox = (await nav.boundingBox())!;
		mainBox = (await main.boundingBox())!;
		expect(navBox.x, 'sidebar sits beside the content').toBeLessThan(mainBox.x);
		expect(navBox.width).toBeCloseTo(216, 0);
		const sideLabel = (await page.locator('.tab-label').first().boundingBox())!;
		const sideMark = (await page.locator('.marks a').first().locator('svg').boundingBox())!;
		expect(sideLabel.width, 'sidebar labels are drawn').toBeGreaterThan(1);
		expect(sideLabel.x, 'sidebar labels sit beside the mark').toBeGreaterThan(sideMark.x);
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
		expect(await tilesPerRow(page), 'tiles at 760px').toBe(2);
		await open(page, '/schedule', WIDTHS.sidebar);
		expect(await columnCount(page, '.canvas'), 'timeline at 760px').toBe(1);

		await open(page, '/', WIDTHS.wide);
		expect(await tilesPerRow(page), 'tiles at 1120px').toBe(4);
		await open(page, '/schedule', WIDTHS.wide);
		expect(await columnCount(page, '.canvas'), 'timeline at 1120px').toBe(2);
	});

	/**
	 * The shelf wraps rather than being a grid, so an odd number of projects
	 * cannot leave a reserved cell with the black background showing through.
	 * The fixture has two projects plus the "new" tile, which is exactly the odd
	 * count that used to draw a tile-sized black square.
	 */
	test('the tile shelf never ends on a hole', async ({ page }) => {
		for (const width of [WIDTHS.phone, WIDTHS.sidebar, WIDTHS.wide, WIDTHS.desktop]) {
			await open(page, '/', width);
			const covered = await page.evaluate(() => {
				const grid = document.querySelector('.grid')!;
				const box = grid.getBoundingClientRect();
				const tiles = [...grid.children].map((el) => el.getBoundingClientRect());
				const rows = new Map<number, number>();
				for (const tile of tiles) {
					const row = Math.round(tile.top);
					rows.set(row, (rows.get(row) ?? 0) + tile.width);
				}
				// Every row's tiles, plus the 2px gaps between them, must span the
				// shelf. A short row means background showing where a tile is not.
				return [...rows.entries()].map(([row, filled]) => ({
					row,
					short:
						box.width - filled - 2 * (tiles.filter((t) => Math.round(t.top) === row).length - 1)
				}));
			});
			for (const row of covered) {
				expect(row.short, `row at ${row.row} on a ${width}px viewport`).toBeLessThan(1);
			}
		}
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

/**
 * Controls that only exist once something has been clicked.
 *
 * `expectWellAligned` walks what is on screen, so an editor behind a button is
 * invisible to every test above — which is exactly where a 22px arrow or a
 * doubled rule would survive. Each of these opens the thing and re-probes at
 * phone width, where the reach rule bites hardest.
 */
test.describe('controls behind a click', () => {
	test('the milestone arrange mode', async ({ page }) => {
		await open(page, '/projects/thesis', WIDTHS.phone);
		await page.getByRole('button', { name: 'Arrange' }).click();
		await expect(page.getByRole('button', { name: /^Move .* up$/ }).first()).toBeVisible();
		await expectWellAligned(page, WIDTHS.phone);
	});

	test('a todo row with its fields open', async ({ page }) => {
		await open(page, '/todos', WIDTHS.phone);

		await page
			.getByRole('button', { name: /^Edit / })
			.first()
			.click();
		await expect(page.getByLabel('Due').first()).toBeVisible();
		await expectWellAligned(page, WIDTHS.phone);
	});

	test('the settled todos are shown', async ({ page }) => {
		await open(page, '/todos', WIDTHS.phone);

		await page.getByRole('button', { name: 'Show settled' }).click();
		await expect(page.getByText('Send the outline')).toBeVisible();
		await expectWellAligned(page, WIDTHS.phone);
	});

	test('the session editor on the log', async ({ page }) => {
		await open(page, '/schedule/log', WIDTHS.phone);
		await page
			.getByRole('button', { name: /^Edit / })
			.first()
			.click();
		await expect(page.getByRole('button', { name: 'Save' })).toBeVisible();
		await expectWellAligned(page, WIDTHS.phone);
	});

	test('the add-time form on the log', async ({ page }) => {
		await open(page, '/schedule/log', WIDTHS.phone);
		await page.getByRole('button', { name: '+ Time by hand' }).click();
		await expect(page.getByLabel('Date')).toBeVisible();
		await expectWellAligned(page, WIDTHS.phone);
	});

	test('the block editor on the day', async ({ page }) => {
		await open(page, '/schedule', WIDTHS.phone);
		await page.getByRole('button', { name: 'Edit Standup', exact: true }).click();
		await expect(page.getByRole('button', { name: 'Save block' })).toBeVisible();
		await expectWellAligned(page, WIDTHS.phone);
	});

	test('a selected project tile', async ({ page }) => {
		await open(page, '/', WIDTHS.phone);
		const tile = page.getByRole('button', { name: /thesis/i });
		await tile.click();
		await expect(tile).toHaveAttribute('aria-pressed', 'true');
		await expectWellAligned(page, WIDTHS.phone);
	});
});

test.describe('hierarchy and identity', () => {
	/**
	 * Alarm-red is for discard and delete. An add action that paints the same
	 * reads as destructive — the week view's + Block used to.
	 */
	test('+ Block is not styled as destructive', async ({ page }) => {
		await open(page, '/schedule/week', WIDTHS.phone);
		const add = page.getByRole('link', { name: '+ Block' });
		const color = await add.evaluate((el) => getComputedStyle(el).backgroundColor);
		expect(color).not.toBe('rgb(209, 51, 46)');
	});

	test('a chosen project is marked on the tile', async ({ page }) => {
		await open(page, '/', WIDTHS.phone);
		const tile = page.getByRole('button', { name: /thesis/i });
		await tile.click();
		const shadow = await tile.evaluate((el) => getComputedStyle(el).boxShadow);
		expect(shadow, 'selected tile has an inset frame').not.toBe('none');
	});

	test('week chips tall enough to hold a title show one', async ({ page }) => {
		await open(page, '/schedule/week', WIDTHS.phone);
		await expect(page.locator('.chip-title', { hasText: 'Deep work' }).first()).toBeVisible();
	});

	/**
	 * One-off rows are buttons so they open the editor. The action-label type
	 * (uppercase, tracked) is what clipped "Evening draft" to "EVENING DRA…".
	 */
	test('a one-off title keeps its written case and is not clipped', async ({ page }) => {
		await open(page, '/schedule', WIDTHS.desktop);
		const title = page.locator('.legend .title', { hasText: 'Evening draft' });
		await expect(title).toBeVisible();
		await expect(title).toHaveCSS('text-transform', 'none');
		const overflow = await title.evaluate((el) => el.scrollWidth - el.clientWidth);
		expect(overflow, 'Evening draft clipped').toBeLessThanOrEqual(1);
	});
});

/** The shelf wraps, so its width is counted from where the tiles actually sit. */
async function tilesPerRow(page: Page): Promise<number> {
	return page.evaluate(() => {
		const tiles = [...document.querySelector('.grid')!.children];
		const first = Math.round(tiles[0].getBoundingClientRect().top);
		return tiles.filter((el) => Math.round(el.getBoundingClientRect().top) === first).length;
	});
}

async function columnCount(page: Page, selector: string): Promise<number> {
	return page.evaluate((sel) => {
		const el = document.querySelector(sel);
		if (!el) throw new Error(`no ${sel} on this screen`);
		return getComputedStyle(el).gridTemplateColumns.split(/\s+/).filter(Boolean).length;
	}, selector);
}
