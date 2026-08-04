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
