/**
 * The probes the layout specs share.
 *
 * Each is the generalisation of a bug this branch has already had to fix by
 * hand, so a regression fails here rather than in someone's eye:
 *
 *   horizontalOverflow — a track that refused to shrink below its content
 *   clippedText        — a flex item squeezed below its own text, which then
 *                        spilled over its neighbour without scrolling the page
 *   croppedText        — a box shorter than the line of text inside it, hiding
 *                        the letters behind its `overflow: hidden` (#17)
 *   unsharedEdges      — "a screen's rules end where its content ends" (2140a19)
 *   doubledRules       — two 2px rules meeting and both drawing (6fc1b2f)
 *   roundedCorners     — the design has no radius token; a radius is a bug
 *   smallTapTargets    — 44px under a thumb, by reach rather than by box
 *
 * `page.evaluate` serialises its callback, so each probe repeats a four-line
 * `label()` rather than sharing one from module scope — and `clippedText` and
 * `croppedText`, which ask the same question on the two axes, each walk the
 * leaves for themselves. That is the cost of running in the page, and it is
 * cheaper than the alternatives: merging them to share the walk would trade six
 * lines for one assertion message where there are now two, and a probe that
 * cannot be pointed at a screen on its own.
 */
import { expect, type Page } from '@playwright/test';

/** Sub-pixel slack. The design aligns on whole pixels; 0.5 is rounding, not drift. */
export const SLACK = 0.5;

export const WIDTHS = {
	phone: 360,
	/** The dead zone: past the 440px shell, still below the sidebar flip. */
	tween: 600,
	/** Just past the sidebar flip — where the half-landed 900px bug lived. */
	sidebar: 760,
	/** Just past the content column reaching its full 900px measure. */
	wide: 1120,
	desktop: 1440
} as const;

export const ALL = Object.values(WIDTHS);
/** Screens with no two-column behaviour prove nothing extra at the middle widths. */
export const ENDS = [WIDTHS.phone, WIDTHS.sidebar, WIDTHS.desktop];

export async function open(page: Page, path: string, width: number): Promise<void> {
	await page.setViewportSize({ width, height: 900 });
	await page.goto(path);
	// The app is an SPA with `ssr = false`: every screen paints only after its
	// fetch resolves, so waiting on `.screen` waits on real content.
	await page.locator('.screen').first().waitFor({ state: 'visible' });
}

export async function horizontalOverflow(page: Page): Promise<string[]> {
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
 *
 * Only *stretched* children are bands. A break screen's ring is a circle with
 * `margin: 0 auto`, and centred content is narrower than its container on
 * purpose — a horizontal margin, or a container that centres its items, is the
 * element saying it is not carrying the screen's edges.
 */
export async function unsharedEdges(page: Page): Promise<string[]> {
	return page.evaluate((slack: number) => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');
		const laidOut = (el: Element) => {
			const box = el.getBoundingClientRect();
			const style = getComputedStyle(el);
			return (
				box.height > 0 &&
				box.width > 0 &&
				style.position === 'static' &&
				parseFloat(style.marginLeft) === 0 &&
				parseFloat(style.marginRight) === 0 &&
				style.alignSelf !== 'center'
			);
		};
		const stretches = (el: Element) => {
			const items = getComputedStyle(el).alignItems;
			return items !== 'center' && items !== 'flex-start' && items !== 'flex-end';
		};

		const bad: string[] = [];
		const columns = [...document.querySelectorAll('.screen')].filter(
			(el) => getComputedStyle(el).flexDirection === 'column' && stretches(el)
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
					stretches(el) &&
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
export async function doubledRules(page: Page): Promise<string[]> {
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
export async function roundedCorners(page: Page): Promise<string[]> {
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
 * This measures *reach*, not the drawn box. The design deliberately draws some
 * controls smaller than a thumb and hands them their 44px through an invisible
 * `::after` overlay — the pattern editor's switch says so outright — and a
 * check that only read `getBoundingClientRect` would call those broken and the
 * overlay useless. Hit-testing the extremes is what the thumb actually does.
 *
 * Two controls are exempt because their size is the design speaking:
 *
 *   .segmented children — "the same construction, smaller" (`app.css:264`)
 *   .block              — a schedule block's height *is* its duration
 *   .day                — a square in a seven-across row; at 360px seven of
 *                         them plus their gaps cannot each be 44 wide, and
 *                         every neighbour is itself a valid target
 *
 * Visually-hidden links are 1px by construction and are skipped too.
 */
export async function smallTapTargets(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');
		const BY_DESIGN = ['block', 'day'];
		/** A pseudo-element hit-tests as its originating element, which is the point. */
		const reaches = (el: Element, x: number, y: number) => {
			const hit = document.elementFromPoint(x, y);
			return !!hit && (hit === el || el.contains(hit) || hit.contains(el));
		};

		const bad: string[] = [];
		for (const el of document.querySelectorAll('button, a[href]')) {
			const box = el.getBoundingClientRect();
			if (box.height === 0 || box.width === 0) continue;
			if (getComputedStyle(el).clipPath !== 'none') continue;
			if (el.closest('.segmented')) continue;
			if (BY_DESIGN.some((name) => el.classList.contains(name))) continue;
			if (box.height >= 44 - 0.5) continue;

			// `elementFromPoint` is viewport-relative and returns null past the
			// fold, so bring the control into view before asking where it is.
			el.scrollIntoView({ block: 'center' });
			const seen = el.getBoundingClientRect();
			const midX = seen.left + seen.width / 2;
			const midY = seen.top + seen.height / 2;
			if (!reaches(el, midX, midY - 21) || !reaches(el, midX, midY + 21)) {
				bad.push(`${label(el)} is ${seen.height.toFixed(1)}px tall and does not reach 44px`);
			}
		}
		return bad;
	});
}

/**
 * Text that does not fit the box it was given.
 *
 * `horizontalOverflow` only sees the document and `main`, so a flex item squeezed
 * below its own content passes it: the text spills over a sibling instead, and
 * the page still does not scroll. That is how a longer total crushed the schedule
 * header's title until the title stopped shrinking.
 *
 * Only leaves are checked — a wrapper's `scrollWidth` is its children's, and a
 * deliberate scroller (a wide table, a code block) opts out through `overflow-x`.
 */
export async function clippedText(page: Page): Promise<string[]> {
	return page.evaluate((slack: number) => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');

		const bad: string[] = [];
		const range = document.createRange();
		for (const el of document.querySelectorAll('.screen *')) {
			if (el.children.length > 0 || !el.textContent?.trim()) continue;
			const style = getComputedStyle(el);
			if (style.overflowX !== 'visible' || style.display === 'none') continue;

			// The text itself, not `scrollWidth` — an absolutely positioned `::after`
			// inflates that, and the settings stepper uses one as its 44px overlay.
			range.selectNodeContents(el);
			const text = range.getBoundingClientRect().width;
			const box = el.getBoundingClientRect().width;
			if (text > box + slack) {
				bad.push(`${label(el)} is ${box.toFixed(0)}px wide for ${text.toFixed(0)}px of text`);
			}
		}
		return bad;
	}, SLACK);
}

/**
 * Text with its top or bottom sliced off by a box that hides its overflow.
 *
 * `clippedText` measures width and so misses the whole vertical half of the same
 * bug. A schedule block's height *is* its duration, which means the box can be
 * shorter than the one line of text inside it — and because a button centres its
 * own contents, the half that went missing was the top of the title (#17).
 *
 * Only `overflow: hidden` counts. A scroller hides nothing: its content is one
 * gesture away.
 *
 * The two halves below are the two ways the letters go missing, and a leaf can
 * only ever be caught by one of them:
 *
 *   an ancestor is too short  — the text spills past it and is clipped
 *   the leaf itself is too short — it clips its own text and nothing spills
 *
 * The second needs the leaf's *layout* height rather than the ink box the first
 * one uses: an element that hides its own overflow does so to carry an ellipsis,
 * which is a promise about width only, and its ink box is a couple of pixels
 * taller than its line box at any font size.
 */
export async function croppedText(page: Page): Promise<string[]> {
	return page.evaluate((slack: number) => {
		const label = (el: Element) =>
			el.tagName.toLowerCase() +
			(typeof el.className === 'string' && el.className.trim()
				? `.${el.className.trim().split(/\s+/).join('.')}`
				: '');

		const bad: string[] = [];
		const range = document.createRange();
		for (const el of document.querySelectorAll('.screen *')) {
			if (el.children.length > 0 || !el.textContent?.trim()) continue;
			const style = getComputedStyle(el);
			if (style.display === 'none' || style.clipPath !== 'none') continue;

			if (style.overflowY === 'hidden') {
				const line = parseFloat(style.lineHeight);
				const own =
					el.getBoundingClientRect().height -
					parseFloat(style.paddingTop) -
					parseFloat(style.paddingBottom) -
					parseFloat(style.borderTopWidth) -
					parseFloat(style.borderBottomWidth);
				if (own < line - slack) {
					bad.push(`${label(el)} is ${own.toFixed(1)}px tall for a ${line.toFixed(1)}px line`);
				}
			}

			range.selectNodeContents(el);
			const text = range.getBoundingClientRect();
			if (text.height === 0) continue;

			for (let box: Element | null = el.parentElement; box; box = box.parentElement) {
				const boxStyle = getComputedStyle(box);
				if (boxStyle.overflowY !== 'hidden') continue;
				// Overflow is clipped at the padding box, so the border is what the
				// text is measured against rather than the outer edge.
				const frame = box.getBoundingClientRect();
				const top = frame.top + parseFloat(boxStyle.borderTopWidth);
				const bottom = frame.bottom - parseFloat(boxStyle.borderBottomWidth);
				const lost = Math.max(top - text.top, text.bottom - bottom);
				if (lost > slack) {
					bad.push(
						`${label(el)} loses ${lost.toFixed(1)}px of ${text.height.toFixed(1)}px ` +
							`of text inside ${label(box)}`
					);
					// One report per leaf: under nested clippers the outer box says
					// nothing the inner one has not already said.
					break;
				}
			}
		}
		return bad;
	}, SLACK);
}

/** Every probe, run against whatever is currently on screen. */
export async function expectWellAligned(page: Page, width: number): Promise<void> {
	expect(await horizontalOverflow(page), 'horizontal overflow').toEqual([]);
	expect(await clippedText(page), 'text overflowing its own box').toEqual([]);
	expect(await croppedText(page), 'text cut off by a box that hides it').toEqual([]);
	expect(await unsharedEdges(page), 'bands do not share an edge').toEqual([]);
	expect(await doubledRules(page), 'two rules drawing where one should').toEqual([]);
	expect(await roundedCorners(page), 'the design has no radius').toEqual([]);
	if (width === WIDTHS.phone) {
		expect(await smallTapTargets(page), 'tap target below 44px').toEqual([]);
	}
}
