/**
 * What a form opens with.
 *
 * The day screen and the log each serve two forms — create and amend — from one
 * set of fields, because both write the same four things and rendering the
 * markup twice is how the two drift. The cost is that whatever an edit leaves
 * behind is what the next "add" opens with unless something puts it back.
 *
 * A separate file from `layout.spec.ts`, which measures geometry and nothing
 * else. These are the only assertions in the suite about what a control *says*
 * rather than where it sits, and there is nowhere cheaper to make them: the
 * route files carry no behavioural unit tests and the vitest coverage gate
 * does not read them.
 *
 * Every field is addressed with `{ exact: true }`, because `getByLabel` matches
 * a substring and each of these screens lists its rows beside the form it is
 * opening. A session with no project is labelled "Edit No project at 16:00",
 * which answers to "Project" — so whether this file passed depended on which
 * day of the week the fixture's Sunday session landed on.
 */
import { expect, test } from '@playwright/test';

import { WIDTHS, open } from './probes';

test('the block form opens empty after an edit was cancelled', async ({ page }) => {
	await open(page, '/schedule', WIDTHS.phone);

	// The seeded one-off differs from the defaults in all four fields, so a
	// field left behind is a field this catches.
	await page.getByRole('button', { name: 'Edit Standup', exact: true }).click();
	await expect(page.getByLabel('Start', { exact: true })).toHaveValue('12:00');
	await expect(page.getByLabel('Title', { exact: true })).toHaveValue('Standup');
	await page.getByRole('button', { name: 'Cancel' }).click();

	await page.getByRole('button', { name: '+ Block' }).click();
	await expect(page.getByRole('button', { name: 'Add block' })).toBeVisible();
	await expect(page.getByLabel('Start', { exact: true })).toHaveValue('09:00');
	await expect(page.getByLabel('End', { exact: true })).toHaveValue('10:00');
	await expect(page.getByLabel('Project', { exact: true })).toHaveValue('');
	await expect(page.getByLabel('Title', { exact: true })).toHaveValue('');
});

test('the add-time form opens empty after an edit was cancelled', async ({ page }) => {
	await open(page, '/schedule/log', WIDTHS.phone);

	await page.getByRole('button', { name: 'Edit Atlas ingest at 11:00' }).first().click();
	await expect(page.getByLabel('Start', { exact: true })).toHaveValue('11:00');
	await expect(page.getByLabel('Note', { exact: true })).toHaveValue(/ingest step/);
	await page.getByRole('button', { name: 'Cancel' }).click();

	await page.getByRole('button', { name: '+ Time by hand' }).click();
	await expect(page.getByLabel('Date', { exact: true })).toBeVisible();
	await expect(page.getByLabel('Start', { exact: true })).toHaveValue('09:00');
	await expect(page.getByLabel('End', { exact: true })).toHaveValue('10:00');
	await expect(page.getByLabel('Project', { exact: true })).toHaveValue('');
	await expect(page.getByLabel('Note', { exact: true })).toHaveValue('');
});

/**
 * Phone chrome: a native date field's min-content width used to eat the
 * composer, leaving the title as a ~40px square whose placeholder could not
 * be read. The title takes the remaining row; the date stays compact.
 */
test('the todos composer gives the title the remaining width', async ({ page }) => {
	for (const width of [WIDTHS.phone, 390, WIDTHS.desktop]) {
		await open(page, '/todos', width);
		const title = page.getByRole('textbox', { name: 'New todo', exact: true });
		const due = page.getByLabel('Due date for the new todo');
		const titleBox = (await title.boundingBox())!;
		const dueBox = (await due.boundingBox())!;
		expect(titleBox.width, `title at ${width}px`).toBeGreaterThan(dueBox.width);
		expect(titleBox.width, `title readable at ${width}px`).toBeGreaterThan(160);
		const overflow = await title.evaluate((el) => el.scrollWidth - el.clientWidth);
		expect(overflow, `placeholder clipped at ${width}px`).toBeLessThanOrEqual(1);
		await expect(title).toHaveAttribute('placeholder', 'Add a todo…');
	}
});

test('the todos composer controls share a height on a wide row', async ({ page }) => {
	await open(page, '/todos', WIDTHS.desktop);
	const title = page.getByRole('textbox', { name: 'New todo', exact: true });
	const due = page.getByLabel('Due date for the new todo');
	const add = page.getByRole('button', { name: 'Add', exact: true });
	const titleBox = (await title.boundingBox())!;
	const dueBox = (await due.boundingBox())!;
	const addBox = (await add.boundingBox())!;
	expect(Math.abs(titleBox.height - dueBox.height)).toBeLessThan(1);
	expect(Math.abs(dueBox.height - addBox.height)).toBeLessThan(1);
	expect(Math.abs(titleBox.y - dueBox.y)).toBeLessThan(1);
	expect(Math.abs(dueBox.y - addBox.y)).toBeLessThan(1);
});

/**
 * A control is clear of the tab bar when a tap on its centre hits it, not ●■◆▲.
 * The add/edit sheets reuse the delete dialog's chrome so this stays true on
 * a 390×844 phone, where the inline form used to sit under the nav.
 */
async function expectClearOfTabBar(
	page: import('@playwright/test').Page,
	control: import('@playwright/test').Locator
) {
	await expect(control).toBeVisible();
	const box = (await control.boundingBox())!;
	const viewport = page.viewportSize()!;
	expect(box.y).toBeGreaterThanOrEqual(0);
	expect(box.y + box.height).toBeLessThanOrEqual(viewport.height + 0.5);
	const hit = await control.evaluate((el) => {
		const seen = el.getBoundingClientRect();
		const target = document.elementFromPoint(
			seen.left + seen.width / 2,
			seen.top + seen.height / 2
		);
		return !!target && (target === el || el.contains(target));
	});
	expect(hit, 'control is under another layer').toBe(true);
}

test('the add-block sheet is fully usable on a phone', async ({ page }) => {
	await open(page, '/schedule', WIDTHS.phone);
	await page.setViewportSize({ width: 390, height: 844 });
	await page.getByRole('button', { name: '+ Block' }).click();
	await expect(page.getByRole('dialog', { name: 'Add block' })).toBeVisible();
	await expect(page.getByLabel('Title', { exact: true })).toBeVisible();
	await expect(page.getByLabel('Project', { exact: true })).toBeVisible();
	await expectClearOfTabBar(page, page.getByRole('button', { name: 'Add block' }));
});

test('the edit-block sheet is fully usable on a phone', async ({ page }) => {
	await open(page, '/schedule', WIDTHS.phone);
	await page.setViewportSize({ width: 390, height: 844 });
	await page.getByRole('button', { name: 'Edit Standup', exact: true }).click();
	await expect(page.getByRole('dialog', { name: 'Edit block' })).toBeVisible();
	await expectClearOfTabBar(page, page.getByRole('button', { name: 'Save block' }));
});

test('the add-time sheet is fully usable on a phone', async ({ page }) => {
	await open(page, '/schedule/log', WIDTHS.phone);
	await page.setViewportSize({ width: 390, height: 844 });
	await page.getByRole('button', { name: '+ Time by hand' }).click();
	await expect(page.getByRole('dialog', { name: 'Log time by hand' })).toBeVisible();
	await expect(page.getByLabel('Date', { exact: true })).toBeVisible();
	await expectClearOfTabBar(page, page.getByRole('button', { name: 'Log it' }));
});

test('skip and restore stay tappable on a phone', async ({ page }) => {
	await open(page, '/schedule', WIDTHS.phone);
	await page.setViewportSize({ width: 390, height: 844 });
	const skip = page
		.getByRole('listitem')
		.filter({ hasText: 'Atlas ingest' })
		.getByRole('button', { name: 'Skip' });
	await skip.scrollIntoViewIfNeeded();
	await expectClearOfTabBar(page, skip);
	await skip.click();
	const restore = page.getByRole('button', { name: 'Restore' });
	await restore.scrollIntoViewIfNeeded();
	await expectClearOfTabBar(page, restore);
});
