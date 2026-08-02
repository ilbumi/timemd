<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import IdentityPicker from '$lib/IdentityPicker.svelte';
	import Mark from '$lib/Mark.svelte';
	import { api, type LoggedSession, type Mark as MarkShape, type Project } from '$lib/api';
	import { attempt, describe } from '$lib/attempt';
	import { formatHours, parseMinutes } from '$lib/countdown';
	import { clockTime, dayLabel, today, weekDates } from '$lib/dates';
	import { DEFAULT_COLOR, contrastInk, paletteColor } from '$lib/palette';
	import {
		readLifetimeTotals,
		targetFill,
		targetMinutes,
		totalsFor,
		type Totals
	} from '$lib/totals';

	/** The stepper's step, in minutes. Half-hours, so a `1h30m` target written by
	    hand survives an edit instead of being rounded to the nearest hour. */
	const TARGET_STEP = 30;
	const MAX_TARGET = 60 * 60;

	const slug = $derived(page.params.slug ?? '');

	let project = $state<Project | null>(null);
	let lifetime = $state<Record<string, Totals>>({});
	let recent = $state<{ date: string; session: LoggedSession }[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let busy = $state(false);

	let editing = $state(false);
	let draftName = $state('');
	let draftMark = $state<MarkShape>('square');
	let draftColor = $state(DEFAULT_COLOR);
	let draftTarget = $state(0);
	let newMilestone = $state('');
	/**
	 * Off by default, and a section-level mode rather than a third control on
	 * each row: the whole row is already the tick target, deliberately, and at
	 * 360px there is no room beside it.
	 */
	let arranging = $state(false);

	let confirming = $state(false);
	let typedName = $state('');

	const dismissDelete = (): void => {
		confirming = false;
		typedName = '';
	};

	const color = $derived(
		project === null ? DEFAULT_COLOR : paletteColor(project.slug, project.color)
	);
	const ink = $derived(contrastInk(color));
	const archived = $derived(project?.status === 'archived');
	/**
	 * This week's total, counted from the sessions already loaded rather than
	 * asked for again: `recent` is exactly this project's week, and the report
	 * endpoint would re-read the same seven day files to say the same thing.
	 */
	const tracked = $derived({
		tracked: recent.reduce((total, entry) => total + parseMinutes(entry.session.duration), 0),
		sessions: recent.length
	});
	const logged = $derived(totalsFor(lifetime, slug));
	const target = $derived(project === null ? 0 : targetMinutes(project));
	const fill = $derived(targetFill(tracked.tracked, target));
	const doneCount = $derived(project?.milestones.filter((milestone) => milestone.done).length ?? 0);
	const canDelete = $derived(
		typedName.trim().toLowerCase() === (project?.name ?? '').trim().toLowerCase()
	);

	async function load(): Promise<void> {
		error = await attempt(async () => {
			project = await api.readProject(slug);
		});
		loading = false;
	}

	/**
	 * This project's sessions for the current week.
	 *
	 * Seven small reads in parallel rather than a new endpoint: the day files are
	 * the only place a session's note lives, and the report endpoint deliberately
	 * returns totals rather than rows.
	 */
	async function loadRecent(): Promise<void> {
		const days = await Promise.all(
			weekDates(today()).map((date) =>
				api
					.readDay(date)
					.then((day) => ({ date, sessions: day.sessions }))
					.catch(() => ({ date, sessions: [] as LoggedSession[] }))
			)
		);

		recent = days
			.flatMap(({ date, sessions }) =>
				sessions.filter((session) => session.project === slug).map((session) => ({ date, session }))
			)
			.reverse();
	}

	function startEditing(): void {
		if (project === null) return;
		draftName = project.name;
		draftMark = project.mark;
		draftColor = color;
		draftTarget = target;
		editing = true;
	}

	const saveEdits = (): Promise<void> =>
		run(async () => {
			project = await api.updateProject(slug, {
				name: draftName.trim(),
				mark: draftMark,
				color: draftColor,
				target: draftTarget === 0 ? null : `${draftTarget}m`
			});
			editing = false;
		});

	const toggleMilestone = (position: number): Promise<void> =>
		run(async () => {
			const current = project;
			if (current === null) return;
			const milestones = current.milestones.map((milestone, index) =>
				index === position ? { ...milestone, done: !milestone.done } : milestone
			);
			project = await api.updateProject(slug, { milestones });
		});

	const addMilestone = (): Promise<void> =>
		run(async () => {
			const current = project;
			const title = newMilestone.trim();
			if (current === null || title === '') return;
			project = await api.updateProject(slug, {
				milestones: [...current.milestones, { done: false, title }]
			});
			newMilestone = '';
		});

	const removeMilestone = (position: number): Promise<void> =>
		run(async () => {
			const current = project;
			if (current === null) return;
			const milestones = current.milestones.filter((_, index) => index !== position);
			project = await api.updateProject(slug, { milestones });
		});

	/**
	 * Commits a retitle, on blur or Enter rather than per keystroke.
	 *
	 * Nothing is sent when the title has not changed, so tabbing through the
	 * list does not write the file once per row.
	 */
	const renameMilestone = (position: number, title: string): Promise<void> =>
		run(async () => {
			const current = project;
			const trimmed = title.trim();
			if (current === null || trimmed === '') return;
			if (trimmed === current.milestones[position]?.title) return;

			const milestones = current.milestones.map((milestone, index) =>
				index === position ? { ...milestone, title: trimmed } : milestone
			);
			project = await api.updateProject(slug, { milestones });
		});

	/**
	 * Moves a milestone one place up or down.
	 *
	 * Arrows rather than drag: a drag handle is a sub-44px grip, and the layout
	 * gate exempts only shapes whose size carries meaning. One press is one
	 * whole-list PATCH, which is exactly what that endpoint is good at.
	 */
	const moveMilestone = (position: number, by: -1 | 1): Promise<void> =>
		run(async () => {
			const current = project;
			const target = position + by;
			if (current === null || target < 0 || target >= current.milestones.length) return;

			const milestones = [...current.milestones];
			const [moved] = milestones.splice(position, 1);
			if (moved === undefined) return;
			milestones.splice(target, 0, moved);
			project = await api.updateProject(slug, { milestones });
		});

	const setStatus = (status: 'active' | 'archived'): Promise<void> =>
		run(async () => {
			project = await api.updateProject(slug, { status });
		});

	const startSession = (): Promise<void> =>
		run(async () => {
			await api.startSession({ kind: 'focus', project: slug });
			await goto('/');
		});

	const destroy = (): Promise<void> =>
		run(async () => {
			await api.deleteProject(slug);
			await goto('/projects');
		});

	async function run(work: () => Promise<void>): Promise<void> {
		busy = true;
		error = null;
		try {
			await work();
		} catch (failure) {
			error = describe(failure);
		} finally {
			busy = false;
		}
	}

	$effect(() => {
		// Re-runs if the slug changes, which it does when navigating between
		// projects without leaving the route.
		void slug;
		void load();
		void loadRecent();
	});

	$effect(() => {
		// A year of day files, and only the archived layout and the delete sheet
		// ever render it — so an active project never pays for it.
		if (archived) void readLifetimeTotals().then((rows) => (lifetime = rows));
	});
</script>

{#if loading}
	<p class="empty">Loading…</p>
{:else if project === null}
	<p class="empty">No project named <code>{slug}</code>.</p>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
{:else}
	{@const current = project}
	<section class="screen">
		<!-- The header and everything else, wrapped so the wide layout has an
		     element to turn into a row: `.screen` is the query container, and an
		     element cannot query itself. -->
		<div class="split">
			<header
				class="head"
				style:background={archived ? 'var(--paper-dim)' : color}
				style:color={archived ? 'var(--ink)' : ink}
			>
				<div class="head-top">
					<a href="/projects" aria-label="Back" style:color="inherit">←</a>
					{#if archived}
						<span class="chip">Archived</span>
					{:else if editing}
						<button class="quiet" style:color="inherit" onclick={saveEdits} disabled={busy}>
							Save
						</button>
					{:else}
						<button class="quiet" style:color="inherit" onclick={startEditing}>Edit</button>
					{/if}
				</div>

				{#if editing}
					<div class="edit">
						<label class="label" for="edit-name" style:color="inherit">Name</label>
						<input id="edit-name" type="text" bind:value={draftName} />

						<span class="label" style:color="inherit">Mark</span>
						<IdentityPicker bind:mark={draftMark} bind:color={draftColor} />

						<label class="label" for="edit-target" style:color="inherit">
							Weekly target — {draftTarget === 0 ? 'none' : formatHours(draftTarget)}
						</label>
						<input
							id="edit-target"
							type="range"
							min="0"
							max={MAX_TARGET}
							step={TARGET_STEP}
							bind:value={draftTarget}
						/>
					</div>
				{:else}
					<div class="title">
						<Mark
							mark={current.mark}
							color={archived ? 'var(--ink)' : ink}
							size={38}
							outline={archived}
						/>
						<h1>{current.name}</h1>
					</div>

					{#if archived}
						<div class="stats">
							<div>
								<strong class="numeric">{formatHours(logged.tracked)}</strong><span>Logged</span>
							</div>
							<div><strong class="numeric">{logged.sessions}</strong><span>Sessions</span></div>
							<div>
								<strong class="numeric">{doneCount}/{current.milestones.length}</strong>
								<span>Milestones</span>
							</div>
						</div>
					{:else if target > 0}
						<p class="progress numeric">
							{formatHours(tracked.tracked)}
							<span>/ {formatHours(target)} this week</span>
						</p>
						<div class="target header-bar">
							<span style:width="{fill}%" style:background="var(--yellow)"></span>
						</div>
					{/if}
				{/if}
			</header>

			<div class="pane">
				{#if error}
					<p class="error" role="alert">{error}</p>
				{/if}

				{#if current.problems.length > 0}
					<div class="problems" role="status">
						<strong>{current.problems.length} line(s) in this file could not be read</strong>
						<ul>
							{#each current.problems as problem (problem)}
								<li>{problem}</li>
							{/each}
						</ul>
					</div>
				{/if}

				<div class="body">
					<div class="section-head">
						<span class="label"
							>Milestones{archived &&
							doneCount === current.milestones.length &&
							current.milestones.length > 0
								? ' — all done'
								: ''}</span
						>
						<span class="meta">{doneCount} / {current.milestones.length}</span>
						{#if !archived && current.milestones.length > 0}
							<button
								class="quiet"
								aria-pressed={arranging}
								onclick={() => (arranging = !arranging)}
							>
								{arranging ? 'Done' : 'Arrange'}
							</button>
						{/if}
					</div>

					<ul class="milestones" class:readonly={archived}>
						{#each current.milestones as milestone, position (position)}
							<li>
								{#if archived}
									<Mark mark="triangle" color="var(--ink)" size={18} outline={!milestone.done} />
									<span class:done={milestone.done}>{milestone.title}</span>
								{:else if arranging}
									<button
										class="quiet move"
										aria-label="Move {milestone.title} up"
										onclick={() => moveMilestone(position, -1)}
										disabled={busy || position === 0}>▲</button
									>
									<button
										class="quiet move"
										aria-label="Move {milestone.title} down"
										onclick={() => moveMilestone(position, 1)}
										disabled={busy || position === current.milestones.length - 1}>▼</button
									>
									<input
										type="text"
										class="retitle"
										aria-label="Title of {milestone.title}"
										value={milestone.title}
										onblur={(event) => void renameMilestone(position, event.currentTarget.value)}
										onkeydown={(event) => {
											if (event.key === 'Enter') {
												event.preventDefault();
												event.currentTarget.blur();
											}
										}}
									/>
									<button
										class="quiet"
										aria-label="Remove {milestone.title}"
										onclick={() => removeMilestone(position)}
										disabled={busy}>×</button
									>
								{:else}
									<button
										class="tick"
										aria-pressed={milestone.done}
										onclick={() => toggleMilestone(position)}
										disabled={busy}
									>
										<Mark
											mark="triangle"
											color={milestone.done ? 'var(--red)' : 'var(--ink)'}
											size={18}
											outline={!milestone.done}
										/>
										<span class:done={milestone.done}>{milestone.title}</span>
									</button>
									<button
										class="quiet"
										aria-label="Remove {milestone.title}"
										onclick={() => removeMilestone(position)}
										disabled={busy}>×</button
									>
								{/if}
							</li>
						{/each}

						{#if !archived}
							<li class="adder">
								<Mark mark="triangle" color="var(--ink-30)" size={16} outline />
								<input
									type="text"
									placeholder="Add a milestone…"
									aria-label="New milestone"
									bind:value={newMilestone}
									onkeydown={(event) => {
										if (event.key === 'Enter') {
											event.preventDefault();
											void addMilestone();
										}
									}}
								/>
								<button
									class="quiet"
									onclick={addMilestone}
									disabled={busy || newMilestone.trim() === ''}
								>
									Add
								</button>
							</li>
						{/if}
					</ul>

					{#if archived}
						<p class="empty">
							No new sessions can be logged while archived. Restore to schedule it again.
						</p>
					{:else}
						<div class="section-head bordered">
							<span class="label">This week</span>
							<span class="meta">{tracked.sessions} sessions</span>
						</div>
						{#if recent.length === 0}
							<p class="empty">Nothing logged against this project this week.</p>
						{:else}
							<ul class="sessions">
								{#each recent as entry (`${entry.date}-${entry.session.index}`)}
									<li>
										<span class="when">{dayLabel(entry.date)}</span>
										<span class="what">{entry.session.note || clockTime(entry.session.start)}</span>
										<span class="how-long numeric">{entry.session.duration}</span>
									</li>
								{/each}
							</ul>
						{/if}
					{/if}
				</div>

				<div class="foot">
					{#if archived}
						<button class="primary wide" onclick={() => setStatus('active')} disabled={busy}>
							Restore project
						</button>
						<button class="danger wide" onclick={() => (confirming = true)} disabled={busy}>
							Delete permanently
						</button>
					{:else}
						<div class="actions">
							<button onclick={() => setStatus('archived')} disabled={busy}>Archive</button>
							<button class="primary grow" onclick={startSession} disabled={busy}
								>Start session</button
							>
						</div>
					{/if}
				</div>
			</div>
		</div>
	</section>

	{#if confirming}
		<!-- 4c: names what is lost and offers archiving as the way out. -->
		<div class="sheet-backdrop">
			<div class="sheet" role="dialog" aria-modal="true" aria-label="Delete {current.name}">
				<header>
					<Mark mark="triangle" color="var(--paper)" size={22} />
					<strong>Delete {current.name}?</strong>
				</header>

				<div class="sheet-body">
					<p>This erases the project and everything logged against it. It cannot be undone.</p>

					<div class="losses">
						<div>
							<Mark mark="square" color="var(--red)" size={11} />
							<span>{logged.sessions} sessions</span>
							<strong class="numeric">{formatHours(logged.tracked)}</strong>
						</div>
						<div>
							<Mark mark="square" color="var(--red)" size={11} />
							<span>Milestones</span>
							<strong class="numeric">{current.milestones.length}</strong>
						</div>
					</div>

					<label class="label" for="confirm">Type the name to confirm</label>
					<input id="confirm" type="text" placeholder={current.name} bind:value={typedName} />
				</div>

				<div class="sheet-foot">
					<button class="danger wide" disabled={!canDelete || busy} onclick={destroy}>
						Delete forever
					</button>
					<div class="actions">
						<button class="quiet" onclick={dismissDelete}>Cancel</button>
						<button onclick={dismissDelete}>Keep archived</button>
					</div>
				</div>
			</div>
		</div>
	{/if}
{/if}

<style>
	.head {
		padding: 14px var(--pad) 18px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.head-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--gap);
		min-height: 24px;
	}

	.head-top a {
		display: flex;
		align-items: center;
		width: var(--tap-target);
		height: var(--tap-target);
		/* Vertical only: a negative left margin bought alignment by hanging the
		   glyph outside the header's padding. */
		margin: -10px 0;
		font-size: 1.25rem;
		line-height: 1;
		text-decoration: none;
	}

	.chip {
		padding: 5px 8px;
		border: 1.5px solid currentColor;
		font-size: 0.65625rem;
		font-weight: 600;
		letter-spacing: 0.18em;
		text-transform: uppercase;
	}

	.title {
		display: flex;
		align-items: flex-end;
		gap: 14px;
		margin-top: 16px;
	}

	.title h1 {
		font-size: 1.875rem;
		font-weight: 600;
		line-height: 0.9;
		overflow-wrap: anywhere;
	}

	.progress {
		margin: 16px 0 0;
		font-size: 1.625rem;
		font-weight: 300;
		line-height: 1;
	}

	.progress span {
		font-size: 0.875rem;
		opacity: 0.7;
	}

	/* The shared trough, heavier and taller against a full-bleed colour field. */
	.header-bar {
		height: 10px;
		margin-top: 9px;
		border-width: var(--rule);
	}

	.stats {
		display: flex;
		gap: 22px;
		margin-top: 16px;
	}

	.stats strong {
		display: block;
		font-size: 1.625rem;
		font-weight: 300;
		line-height: 1;
	}

	.stats span {
		display: block;
		margin-top: 5px;
		font-size: 0.65625rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--ink-60);
	}

	.edit {
		display: flex;
		flex-direction: column;
		gap: 10px;
		margin-top: 16px;
	}

	.edit .label {
		margin-bottom: 0;
	}

	/*
	 * The app's one native control. Left to the browser it drew a rounded track
	 * and a round thumb in the browser's own blue — the only radius on the
	 * screen and the only colour not in the palette, sitting inside an otherwise
	 * correct 2px box.
	 *
	 * Redrawn as the trough-and-fill the weekly target already uses everywhere
	 * else, with a square thumb. `currentColor` because this sits on the
	 * project's coloured header, which is the same reason `.target` uses it.
	 */
	.edit input[type='range'] {
		appearance: none;
		padding: 0;
		border: none;
		background: none;
		/* The global form rule pins inputs to `--ink`; on a coloured header the
		   trough has to take the header's own ink like `.target` does. */
		color: inherit;
	}

	.edit input[type='range']::-webkit-slider-runnable-track {
		height: 8px;
		border: 1px solid currentColor;
	}

	.edit input[type='range']::-webkit-slider-thumb {
		appearance: none;
		width: 12px;
		height: 22px;
		/* Half the thumb, less half the track, to sit it on the centre line. */
		margin-top: -8px;
		background: currentColor;
	}

	.edit input[type='range']::-moz-range-track {
		height: 8px;
		border: 1px solid currentColor;
	}

	.edit input[type='range']::-moz-range-thumb {
		width: 12px;
		height: 22px;
		border: none;
		border-radius: 0;
		background: currentColor;
	}

	/* Carries the screen's whole column so the wide layout has something to turn
	   into a row. `.screen` cannot do it itself: it is the query container, and
	   an element cannot query its own container. */
	.split {
		display: flex;
		flex-direction: column;
		flex: 1;
	}

	/* Wraps everything that is not the header, so the desktop grid has two
	   children to place rather than five. */
	.pane {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}

	.body {
		flex: 1;
	}

	.section-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--gap);
		padding: 16px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.section-head.bordered {
		border-top: var(--rule) solid var(--ink);
	}

	.milestones {
		list-style: none;
		margin: 0;
		padding: 4px var(--pad) 0;
	}

	.milestones.readonly {
		opacity: 0.6;
	}

	.milestones li {
		display: flex;
		align-items: center;
		gap: 12px;
		border-bottom: 1px solid var(--ink-15);
	}

	.milestones li > span {
		flex: 1;
		padding: 11px 0;
		font-size: 0.875rem;
	}

	/* The whole row is the target: ticking a milestone one-handed should not
	   need a 20px checkbox. */
	.tick {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
		padding: 11px 0;
		border: none;
		background: none;
		text-align: left;
		text-transform: none;
		letter-spacing: 0;
		font-size: 0.875rem;
	}

	.done {
		color: var(--ink-45);
		text-decoration: line-through;
	}

	.adder input {
		flex: 1;
		min-height: 40px;
		padding: 0;
		border: none;
		background: none;
		font-size: 0.875rem;
	}

	/*
	 * Side by side at a full 44px each, not stacked: stacked they would be
	 * ~22px and would need the reach-overlay trick, and the layout gate is
	 * measuring a thumb, not a cursor.
	 *
	 * The row already draws a border-bottom and nothing here adds a top one —
	 * two rules meeting is what the gate flags.
	 */
	.move {
		flex: none;
		width: 44px;
		min-height: 44px;
		font-size: 0.75rem;
	}

	.retitle {
		flex: 1;
		min-width: 0;
		min-height: 44px;
		padding: 0;
		border: none;
		background: none;
		font-size: 0.875rem;
	}

	.sessions {
		list-style: none;
		margin: 0;
		padding: 0 var(--pad);
	}

	.sessions li {
		display: flex;
		align-items: baseline;
		gap: 12px;
		padding: 11px 0;
		border-bottom: 1px solid var(--ink-15);
	}

	.when {
		flex: none;
		width: 62px;
		font-size: 0.75rem;
		color: var(--ink-45);
	}

	.what {
		flex: 1;
		min-width: 0;
		font-size: 0.8125rem;
		color: var(--ink-80);
	}

	.how-long {
		flex: none;
		font-size: 0.75rem;
		color: var(--ink-45);
	}

	.foot {
		display: flex;
		flex-direction: column;
		gap: 10px;
		border-top: var(--rule) solid var(--ink);
	}

	.grow {
		flex: 1.6 !important;
	}

	/* ---- 4c: the delete sheet -------------------------------------------- */

	.sheet-backdrop {
		position: fixed;
		inset: 0;
		display: flex;
		align-items: flex-end;
		justify-content: center;
		background: rgba(17, 17, 17, 0.55);
	}

	.sheet {
		width: 100%;
		max-width: 440px;
		max-height: 100%;
		overflow-y: auto;
		background: var(--paper);
		border-top: var(--rule) solid var(--ink);
		padding-bottom: env(safe-area-inset-bottom);
	}

	.sheet > header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 15px var(--pad);
		border-bottom: var(--rule) solid var(--ink);
		background: var(--red);
		color: var(--paper);
	}

	.sheet > header strong {
		font-size: 0.9375rem;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}

	.sheet-body {
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding: 18px var(--pad) 0;
	}

	.sheet-body p {
		margin: 0;
		font-size: 0.875rem;
		color: var(--ink-80);
	}

	.losses {
		border: var(--rule) solid var(--ink);
	}

	.losses > div {
		display: flex;
		align-items: center;
		gap: 11px;
		padding: 11px 13px;
		font-size: 0.84375rem;
	}

	.losses > div + div {
		border-top: 1px solid var(--ink-15);
	}

	.losses span {
		flex: 1;
	}

	.sheet-foot {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 16px var(--pad) 18px;
	}

	.sheet-foot .actions {
		border: none;
	}

	.sheet-foot .actions > button {
		border: var(--rule) solid var(--ink);
	}

	.sheet-foot .actions > button.quiet {
		border-color: transparent;
	}

	.sheet-foot .actions > button + button {
		margin-left: 10px;
	}

	/* ---- wide ------------------------------------------------------------ */

	@container screen (min-width: 900px) {
		/*
		 * The header stops being a banner and becomes a panel beside the lists:
		 * the identity and the week's progress stay in view while you work down
		 * the milestones, which is the whole point of having the width.
		 */
		.split {
			flex-direction: row;
			align-items: stretch;
		}

		.head {
			flex: none;
			width: 320px;
			border-bottom: none;
			border-right: var(--rule) solid var(--ink);
		}

		.foot {
			flex-direction: row;
		}

		.foot > .wide,
		.foot > .actions {
			flex: 1;
			max-width: 560px;
		}

		.title h1 {
			font-size: 2.25rem;
		}
	}
</style>
