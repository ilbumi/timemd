<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import IdentityPicker from '$lib/IdentityPicker.svelte';
	import Mark from '$lib/Mark.svelte';
	import {
		api,
		type LoggedSession,
		type Mark as MarkShape,
		type Milestone,
		type Project
	} from '$lib/api';
	import { attempt, describe } from '$lib/attempt';
	import { formatHours, parseMinutes } from '$lib/countdown';
	import { clockTime, dayLabel, shiftDays, startOfWeek, today } from '$lib/dates';
	import { contrastInk, paletteColor } from '$lib/palette';
	import { readTotals, totalsFor, type Totals } from '$lib/totals';

	const LIFETIME_DAYS = 365;
	const DAYS_IN_WEEK = 7;

	const slug = $derived(page.params.slug ?? '');

	let project = $state<Project | null>(null);
	let week = $state<Record<string, Totals>>({});
	let lifetime = $state<Record<string, Totals>>({});
	let recent = $state<{ date: string; session: LoggedSession }[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let busy = $state(false);

	let editing = $state(false);
	let draftName = $state('');
	let draftMark = $state<MarkShape>('square');
	let draftColor = $state('#245a8d');
	let draftTarget = $state(0);
	let newMilestone = $state('');

	let confirming = $state(false);
	let typedName = $state('');

	const color = $derived(project === null ? '#245a8d' : paletteColor(project.slug, project.color));
	const ink = $derived(contrastInk(color));
	const archived = $derived(project?.status === 'archived');
	const tracked = $derived(totalsFor(week, slug));
	const logged = $derived(totalsFor(lifetime, slug));
	const target = $derived(project?.target === null ? 0 : parseMinutes(project?.target ?? ''));
	const fill = $derived(target === 0 ? 0 : Math.min(100, (tracked.tracked / target) * 100));
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
		const monday = startOfWeek(today());
		const dates = Array.from({ length: DAYS_IN_WEEK }, (_, offset) => shiftDays(monday, offset));

		const days = await Promise.all(
			dates.map((date) =>
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
		draftTarget = Math.round(target / 60);
		editing = true;
	}

	const saveEdits = (): Promise<void> =>
		run(async () => {
			project = await api.updateProject(slug, {
				name: draftName.trim(),
				mark: draftMark,
				color: draftColor,
				target: draftTarget === 0 ? null : `${draftTarget}h`
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
		const monday = startOfWeek(today());
		void readTotals(monday, shiftDays(monday, 6)).then((rows) => {
			week = rows;
		});
		void readTotals(shiftDays(today(), -LIFETIME_DAYS), today()).then((rows) => {
			lifetime = rows;
		});
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
						Weekly target — hours, 0 for none
					</label>
					<input id="edit-target" type="number" min="0" max="60" bind:value={draftTarget} />
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
					<div class="bar" style:border-color={ink}>
						<span style:width="{fill}%" style:background="var(--yellow)"></span>
					</div>
				{/if}
			{/if}
		</header>

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
			</div>

			<ul class="milestones" class:readonly={archived}>
				{#each current.milestones as milestone, position (position)}
					<li>
						{#if archived}
							<Mark mark="triangle" color="var(--ink)" size={18} outline={!milestone.done} />
							<span class:done={milestone.done}>{milestone.title}</span>
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
					<button class="primary grow" onclick={startSession} disabled={busy}>Start session</button>
				</div>
			{/if}
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
						<button
							class="quiet"
							onclick={() => {
								confirming = false;
								typedName = '';
							}}>Cancel</button
						>
						<button
							onclick={() => {
								confirming = false;
								typedName = '';
							}}>Keep archived</button
						>
					</div>
				</div>
			</div>
		</div>
	{/if}
{/if}

<style>
	.screen {
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}

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

	.bar {
		height: 10px;
		margin-top: 9px;
		border: var(--rule) solid;
		overflow: hidden;
	}

	.bar > span {
		display: block;
		height: 100%;
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
		padding: 12px var(--pad) 16px;
		border-top: var(--rule) solid var(--ink);
	}

	.wide {
		width: 100%;
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
</style>
