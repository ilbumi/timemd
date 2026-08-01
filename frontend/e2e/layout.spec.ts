/**
 * Alignment and adaptive-layout assertions, run at every breakpoint.
 *
 * Each check below is the generalisation of a bug this branch has already had
 * to fix by hand, so a regression fails here rather than in someone's eye:
 *
 *   overflow      — a track that refused to shrink below its content
 *   unsharedEdges — "a screen's rules end where its content ends" (2140a19)
 *   doubledRules  — two 2px rules meeting and both drawing (6fc1b2f)
 *   roundedCorners— the design has no radius token at all; a radius is a bug
 *   tapTargets    — 44px on a phone, which the 700px block deliberately relaxes
 *
 * `page.evaluate` serialises its callback, so each probe repeats a four-line
 * `label()` rather than sharing one from module scope. That is the cost of
 * running in the page, and it is cheaper than the alternatives.
 */
import { test, expect, type Page } from '@playwright/test';

/** Sub-pixel slack. The design aligns on whole pixels; 0.5 is rounding, not drift. */
const SLACK = 0.5;

const WIDTHS = {
	phone: 360,
	/** The dead zone: past the 440px shell, still below the sidebar flip. */
	tween: 600,
	/** Just past the sidebar flip — where the half-landed 900px bug lived. */
	sidebar: 760,
	/** Just past the content column reaching its full 900px measure. */
	wide: 1120,
	desktop: 1440
} as const;

const ALL = Object.values(WIDTHS);
/** Screens with no two-column behaviour prove nothing extra at the middle widths. */
const ENDS = [WIDTHS.phone, WIDTHS.sidebar, WIDTHS.desktop];

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

async function open(page: Page, path: string, width: number): Promise<void> {
	await page.setViewportSize({ width, height: 900 });
	await page.goto(path);
	// The app is an SPA with `ssr = false`: every screen paints only after its
	// fetch resolves, so waiting on `.screen` waits on real content.
	await page.locator('.screen').first().waitFor({ state: 'visible' });
}

async function horizontalOverflow(page: Page): Promise<string[]> {
	return page.evaluate((slack: number) => {
		const bad: string[] = [];
		const root = document.scrollingElement as HTMLElement;
		if (root.scrollWidth > root.clientWidth + slack) {
			bad.push(`document scrolls horizontally: ${root.scrollWidth} > ${root.clientWidth}`);
		}
		const main = document.querySelector('main');
		if (main && main.scrollWidth > main.clientWidth + slack) {
			bad.push(`main scrolls horizontally: ${main.scrollWidth} > ${main.clientWidth}`);
		}
		return bad;
	}, SLACK);
}

/**
 * Every band of a column screen — its bar, its body, its footer — must begin
 * and end on the same two edges, so the rules that bracket them stop where the
 * content between them stops.
 *
 * Bands nest: `projects/[slug]` wraps everything below its header in a `.pane`
 * so the wide layout has two children to place, and that pane's own children
 * are bands too. So this descends through any full-width column wrapper rather
 * than only looking one level down.
 */
async function unsharedEdges(page: Page): Promise<string[]> {
	return page.evaluate((slack: number) => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');
		const laidOut = (el: Element) => {
			const box = el.getBoundingClientRect();
			const style = getComputedStyle(el);
			return box.height > 0 && box.width > 0 && style.position === 'static';
		};

		const bad: string[] = [];
		const columns = [...document.querySelectorAll('.screen')].filter(
			(el) => getComputedStyle(el).flexDirection === 'column'
		);
		while (columns.length) {
			const parent = columns.pop()!;
			const bands = [...parent.children].filter(laidOut);
			if (bands.length < 2) continue;
			const boxes = bands.map((el) => ({ el, box: el.getBoundingClientRect() }));
			const left = Math.min(...boxes.map((entry) => entry.box.left));
			const right = Math.max(...boxes.map((entry) => entry.box.right));
			for (const { el, box } of boxes) {
				if (Math.abs(box.left - left) > slack) {
					bad.push(`${label(el)} starts at ${box.left.toFixed(1)}, siblings at ${left.toFixed(1)}`);
				}
				if (Math.abs(box.right - right) > slack) {
					bad.push(`${label(el)} ends at ${box.right.toFixed(1)}, siblings at ${right.toFixed(1)}`);
				}
				// A wrapper that fills its parent is carrying bands of its own.
				const style = getComputedStyle(el);
				if (
					style.display === 'flex' &&
					style.flexDirection === 'column' &&
					Math.abs(box.width - (right - left)) < slack
				) {
					columns.push(el);
				}
			}
		}
		return bad;
	}, SLACK);
}

/**
 * Where two bordered boxes touch, exactly one of them may draw the line. The
 * design merges the two by offsetting one back over the other; two rules that
 * both draw read as a 4px weight the sheet does not have.
 */
async function doubledRules(page: Page): Promise<string[]> {
	return page.evaluate((slack: number) => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');
		const thickness = (style: CSSStyleDeclaration, side: string) =>
			parseFloat(style.getPropertyValue(`border-${side}-width`)) || 0;
		const draws = (style: CSSStyleDeclaration, side: string) =>
			thickness(style, side) > 0 &&
			style.getPropertyValue(`border-${side}-style`) !== 'none' &&
			!/rgba\(0, 0, 0, 0\)|transparent/.test(style.getPropertyValue(`border-${side}-color`));

		const bad: string[] = [];
		for (const parent of document.querySelectorAll('body *')) {
			const kids = [...parent.children].filter((el) => {
				const box = el.getBoundingClientRect();
				return box.height > 0 && box.width > 0 && getComputedStyle(el).position === 'static';
			});
			for (let i = 1; i < kids.length; i++) {
				const first = kids[i - 1];
				const second = kids[i];
				const boxA = first.getBoundingClientRect();
				const boxB = second.getBoundingClientRect();
				const styleA = getComputedStyle(first);
				const styleB = getComputedStyle(second);
				if (
					Math.abs(boxB.top - boxA.bottom) < slack &&
					draws(styleA, 'bottom') &&
					draws(styleB, 'top')
				) {
					bad.push(
						`${label(first)} bottom rule meets ${label(second)} top rule ` +
							`(${thickness(styleA, 'bottom')}px + ${thickness(styleB, 'top')}px)`
					);
				}
				if (
					Math.abs(boxB.left - boxA.right) < slack &&
					draws(styleA, 'right') &&
					draws(styleB, 'left')
				) {
					bad.push(
						`${label(first)} right rule meets ${label(second)} left rule ` +
							`(${thickness(styleA, 'right')}px + ${thickness(styleB, 'left')}px)`
					);
				}
			}

			// A parent's rule against its first child's, with no padding between
			// them to keep them apart. This is how a footer that lost its padding
			// doubles the line above its action bar.
			const style = getComputedStyle(parent);
			const first = kids[0];
			const last = kids[kids.length - 1];
			if (first && draws(style, 'top') && draws(getComputedStyle(first), 'top')) {
				const gap = first.getBoundingClientRect().top - parent.getBoundingClientRect().top;
				if (gap - thickness(style, 'top') < slack) {
					bad.push(`${label(parent)} top rule meets its first child ${label(first)}'s top rule`);
				}
			}
			if (last && draws(style, 'bottom') && draws(getComputedStyle(last), 'bottom')) {
				const gap = parent.getBoundingClientRect().bottom - last.getBoundingClientRect().bottom;
				if (gap - thickness(style, 'bottom') < slack) {
					bad.push(
						`${label(parent)} bottom rule meets its last child ${label(last)}'s bottom rule`
					);
				}
			}
		}
		return bad;
	}, SLACK);
}

/**
 * "Nothing is rounded: the radius token is gone rather than set to zero, so a
 * stray `border-radius` reads as a mistake."
 *
 * A full 50% on every corner is not a rounded corner, it is a circle — the
 * design's own first shape, and how the dial, its face, the break ring and the
 * now-dot are drawn. Anything else is the mistake the sheet is talking about.
 */
async function roundedCorners(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');

		const bad: string[] = [];
		for (const el of document.querySelectorAll('body *')) {
			const style = getComputedStyle(el);
			const corners = [
				style.borderTopLeftRadius,
				style.borderTopRightRadius,
				style.borderBottomRightRadius,
				style.borderBottomLeftRadius
			];
			const square = corners.every((corner) => corner === '' || corner === '0px');
			const circle = corners.every((corner) => corner === '50%');
			if (!square && !circle) {
				bad.push(`${label(el)} has border-radius ${corners.join(' ')}`);
			}
		}
		return bad;
	});
}

/**
 * A thumb needs `--tap-target`. The 700px block relaxes it for a pointer, so
 * this only runs on the phone.
 *
 * Some controls are exempt because their size is the design speaking rather
 * than an oversight:
 *
 *   .segmented children — "the same construction, smaller" (`app.css:264`)
 *   .toggle             — 26px by design, reaching 44 via an `::after` overlay
 *   .block              — a schedule block's height *is* its duration
 *   .day                — a square in a seven-across row; at 360px seven of
 *                         them plus their gaps cannot each be 44 wide, and
 *                         every neighbour is itself a valid target
 *
 * Visually-hidden links are 1px by construction and are skipped too.
 */
async function smallTapTargets(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');
		const BY_DESIGN = ['toggle', 'block', 'day'];

		const bad: string[] = [];
		for (const el of document.querySelectorAll('button, a[href]')) {
			const box = el.getBoundingClientRect();
			if (box.height === 0 || box.width === 0) continue;
			if (getComputedStyle(el).clipPath !== 'none') continue;
			if (el.closest('.segmented')) continue;
			if (BY_DESIGN.some((name) => el.classList.contains(name))) continue;
			if (box.height < 44 - 0.5) bad.push(`${label(el)} is ${box.height.toFixed(1)}px tall`);
		}
		return bad;
	});
}

for (const screen of SCREENS) {
	for (const width of screen.widths) {
		test(`${screen.name} @ ${width}px`, async ({ page }) => {
			await open(page, screen.path, width);

			expect(await horizontalOverflow(page), 'horizontal overflow').toEqual([]);
			expect(await unsharedEdges(page), 'bands do not share an edge').toEqual([]);
			expect(await doubledRules(page), 'two rules drawing where one should').toEqual([]);
			expect(await roundedCorners(page), 'the design has no radius').toEqual([]);
			if (width === WIDTHS.phone) {
				expect(await smallTapTargets(page), 'tap target below 44px').toEqual([]);
			}
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
