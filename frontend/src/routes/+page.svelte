<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import {
		api,
		type LoggedSession,
		type Project,
		type Running,
		type Settings,
		type TimerState
	} from '$lib/api';
	import { describe } from '$lib/attempt';
	import { Countdown, formatClock, formatHours, progress } from '$lib/countdown';
	import { today } from '$lib/dates';
	import { contrastInk, paletteColor } from '$lib/palette';
	import { readWeekTotals, targetFill, targetMinutes, totalsFor, type Totals } from '$lib/totals';

	/** How often to re-ask the server while the screen is visible. */
	const POLL_MS = 20_000;
	/** How often to redraw the countdown between polls. */
	const TICK_MS = 250;

	const countdown = new Countdown();

	let timer = $state<TimerState | null>(null);
	let projects = $state<Project[]>([]);
	let weekly = $state<Record<string, Totals>>({});
	let settings = $state<Settings | null>(null);
	let loadingProjects = $state(true);
	let error = $state<string | null>(null);
	let busy = $state(false);
	let seconds = $state(0);
	let chosenProject = $state('');
	let note = $state('');
	/** Kept apart from `note`: the first-run screen asks for a project name, not
	    a note, and sharing one variable made two unrelated meanings of it. */
	let firstProjectName = $state('');

	/**
	 * The screen shown after a focus block is logged — the design's one moment
	 * worth interrupting for. Held separately from the timer state because the
	 * server has already moved on to idle by the time we can show it.
	 */
	let finished = $state<{ session: LoggedSession; date: string } | null>(null);
	let finishedNote = $state('');

	const running = $derived(timer?.active ?? null);
	const spent = $derived(progress(seconds, running?.durationSeconds ?? 0));
	const firstRun = $derived(!loadingProjects && projects.length === 0);

	const chosen = $derived(projects.find((project) => project.slug === chosenProject) ?? null);
	const finishedProject = $derived(
		projects.find((project) => project.slug === finished?.session.project) ?? null
	);
	/** The next milestone worth ticking off, if the project keeps any. */
	const nextMilestone = $derived.by(() => {
		const list = finishedProject?.milestones ?? [];
		const index = list.findIndex((milestone) => !milestone.done);
		const milestone = list[index ?? -1];
		return milestone === undefined ? null : { index, milestone };
	});

	function tracked(project: Project): number {
		return totalsFor(weekly, project.slug).tracked;
	}

	async function run(work: () => Promise<TimerState>): Promise<void> {
		busy = true;
		error = null;
		try {
			apply(await work());
		} catch (failure) {
			error = describe(failure);
			// Drop the anchor: otherwise `elapsed()` stays true against a deadline
			// the server never confirmed, and the tick re-polls every 250ms forever.
			countdown.sync(null, performance.now());
			seconds = 0;
		} finally {
			busy = false;
		}
	}

	function apply(next: TimerState): void {
		// A focus block that was running and has now been logged is the trigger for
		// the completion screen. Reading it off `completedToday` rather than off a
		// flag means the server's own tick — which fires while the tab is asleep —
		// counts too, not just a stop we asked for.
		const finishedFocus =
			timer !== null &&
			timer.active?.kind === 'focus' &&
			next.active === null &&
			next.completedToday > timer.completedToday
				? timer.active
				: null;

		timer = next;
		countdown.sync(next.active?.remainingSeconds ?? null, performance.now());
		seconds = countdown.remaining(performance.now());
		if (next.active) {
			chosenProject = next.active.project ?? '';
			note = next.active.note;
		}
		if (finishedFocus !== null) void openFinished(finishedFocus);
	}

	/**
	 * Finds the block that was just written so its note can still be edited.
	 *
	 * Matched on its start time rather than taken as the last row: sessions are
	 * stored sorted by start, so a meeting logged by hand for later in the day
	 * would otherwise be the one whose note this screen edits.
	 */
	async function openFinished(block: Running): Promise<void> {
		const date = today();
		const startedAt = block.startedAt.slice(11, 19);
		try {
			const day = await api.readDay(date);
			const session =
				day.sessions.findLast((candidate) => candidate.start === startedAt) ?? day.sessions.at(-1);
			if (session === undefined) return;
			finished = { session, date };
			finishedNote = session.note;
			note = '';
		} catch {
			// Missing the celebration is not worth an error banner; the session is
			// logged either way and the log screen will show it.
		}
	}

	async function dismissFinished(): Promise<void> {
		const closing = finished;
		if (closing === null) return;

		const trimmed = finishedNote.trim();
		if (trimmed !== closing.session.note) {
			error = null;
			try {
				await api.updateSession(closing.date, closing.session.index, {
					start: closing.session.start,
					end: closing.session.end,
					project: closing.session.project,
					note: trimmed
				});
			} catch (failure) {
				error = describe(failure);
				return;
			}
		}
		finished = null;
	}

	async function tickMilestone(): Promise<void> {
		const project = finishedProject;
		const next = nextMilestone;
		if (project === null || next === null) return;

		const milestones = project.milestones.map((milestone, index) =>
			index === next.index ? { ...milestone, done: true } : milestone
		);
		error = null;
		try {
			replace(await api.updateProject(project.slug, { milestones }));
		} catch (failure) {
			error = describe(failure);
		}
	}

	function replace(updated: Project): void {
		projects = projects.map((project) => (project.slug === updated.slug ? updated : project));
	}

	const refresh = (): Promise<void> => run(() => api.readTimer());

	const startFocus = (): Promise<void> =>
		run(() =>
			api.startSession({ kind: 'focus', project: chosenProject || null, note: note.trim() })
		);

	const stop = (): Promise<void> => run(() => api.stopSession());

	const discard = (): Promise<void> => run(() => api.cancelSession());

	async function takeBreak(): Promise<void> {
		const kind = timer?.nextBreakKind ?? 'short_break';
		await dismissFinished();
		await run(() => api.startSession({ kind }));
	}

	async function begin(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const name = firstProjectName.trim();
		if (name === '') return;
		error = null;
		try {
			const created = await api.createProject({ name });
			projects = [created];
			chosenProject = created.slug;
			firstProjectName = '';
		} catch (failure) {
			error = describe(failure);
		}
	}

	async function loadProjects(): Promise<void> {
		try {
			projects = await api.listActiveProjects();
		} catch {
			// The timer is usable without the project list, so a failure here should
			// not take the screen down with it.
		} finally {
			loadingProjects = false;
		}
	}

	async function loadWeek(): Promise<void> {
		weekly = await readWeekTotals();
	}

	$effect(() => {
		void refresh();
		void loadProjects();
		void loadWeek();
		api
			.readSettings()
			.then((loaded) => {
				settings = loaded;
			})
			.catch(() => {
				// The hero falls back to the stock 25m; nothing else needs it.
			});
	});

	$effect(() => {
		// Tick locally between polls, and re-ask the server the moment the deadline
		// passes so the finished session is picked up promptly.
		const tick = setInterval(() => {
			seconds = countdown.remaining(performance.now());
			// Clear the anchor before re-polling, so one refresh is dispatched per
			// completed session rather than one per tick until it returns.
			if (!busy && countdown.elapsed(performance.now())) {
				countdown.sync(null, performance.now());
				void refresh();
			}
		}, TICK_MS);

		const poll = setInterval(() => {
			if (!busy && document.visibilityState === 'visible') void refresh();
		}, POLL_MS);

		// A phone that has been asleep needs a fresh answer the instant it is
		// looked at, not up to a poll interval later.
		const onVisible = (): void => {
			if (document.visibilityState === 'visible') void refresh();
		};
		document.addEventListener('visibilitychange', onVisible);

		return () => {
			clearInterval(tick);
			clearInterval(poll);
			document.removeEventListener('visibilitychange', onVisible);
		};
	});
</script>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}

{#if firstRun}
	<!-- 3k: no account, no tour. One project and you are timing. -->
	<section class="screen welcome">
		<div class="logo">
			<Mark mark="square" color="var(--red)" size={26} />
			<Mark mark="circle" color="var(--blue)" size={26} />
			<Mark mark="triangle" color="var(--yellow)" size={28} />
		</div>

		<div class="pitch">
			<h1>TWENTY<br />FIVE<br /><strong>MINUTES</strong></h1>
			<div class="rule"></div>
			<p>
				Name one thing you are working on. Everything else — schedule, milestones, history — grows
				from it.
			</p>
		</div>

		<form onsubmit={begin}>
			<label class="label" for="first-project">First project</label>
			<input
				id="first-project"
				class="underlined"
				type="text"
				placeholder="e.g. Thesis"
				bind:value={firstProjectName}
			/>

			<div class="spacer"></div>

			<div class="actions">
				<button class="primary" type="submit" disabled={firstProjectName.trim() === ''}
					>Begin</button
				>
			</div>
			<p class="footnote">No account. Everything stays in markdown on your server.</p>
		</form>
	</section>
{:else if finished}
	<!-- 3b: log a note, tick a milestone. -->
	{@const done = finished}
	<section class="screen finished">
		<header class="complete">
			<div class="eyebrow">Session complete</div>
			<div class="headline">
				<strong class="numeric">{done.session.duration}</strong>
				{#if finishedProject}
					<span class="named">
						<Mark mark={finishedProject.mark} color="var(--paper)" size={15} />
						{finishedProject.name}
					</span>
				{/if}
			</div>
		</header>

		<div class="body">
			<div class="field">
				<label class="label" for="finished-note">What got done</label>
				<textarea id="finished-note" rows="3" bind:value={finishedNote}></textarea>
			</div>

			{#if nextMilestone}
				<div class="field">
					<div class="label">Milestone — {nextMilestone.milestone.title}</div>
					<button class="milestone" onclick={tickMilestone}>
						<Mark mark="triangle" color="var(--red)" size={18} />
						<span>Mark as done</span>
						<span class="box"></span>
					</button>
				</div>
			{/if}

			{#if finishedProject}
				{@const goal = targetMinutes(finishedProject)}
				{#if goal > 0}
					<div class="week">
						<span class="hairline"></span>
						<span class="numeric">
							Week: {formatHours(tracked(finishedProject))} / {formatHours(goal)}
						</span>
					</div>
				{/if}
			{/if}
		</div>

		<div class="foot">
			<div class="actions">
				<button onclick={dismissFinished}>Done</button>
				<button class="primary" onclick={takeBreak} disabled={busy}>Take break</button>
			</div>
		</div>
	</section>
{:else if running && running.kind !== 'focus'}
	<!-- 3a: the field inverts so it is unmistakably not work time. -->
	{@const current = running}
	<section class="screen break">
		<header class="bar">
			<div class="eyebrow">{current.kind === 'long_break' ? 'Long break' : 'Short break'}</div>
			<div class="meta">{timer?.completedToday ?? 0} done today</div>
		</header>

		<div class="middle">
			<div class="ring">
				<strong class="numeric">{formatClock(seconds)}</strong>
				<span>Stand up</span>
			</div>
			<p>Ends on its own — the app logs and notifies even with the screen off.</p>
		</div>

		<div class="foot">
			<div class="actions">
				<button class="primary" onclick={discard} disabled={busy}>Skip break</button>
			</div>
		</div>
	</section>
{:else if running}
	<!-- 2a: the circle is the session. -->
	{@const current = running}
	<section class="screen">
		<header class="bar">
			<div class="eyebrow">
				Focus {((timer?.completedToday ?? 0) + 1).toString().padStart(2, '0')}
			</div>
			<div class="logo small">
				<Mark mark="square" color="var(--red)" size={11} />
				<Mark mark="square" color="var(--blue)" size={11} />
				<Mark mark="square" color="var(--yellow)" size={11} />
			</div>
		</header>

		<div class="middle">
			<div class="dial" style:--spent={spent}>
				<div class="face">
					<strong class="numeric">{formatClock(seconds)}</strong>
					<span>of {current.duration}</span>
				</div>
				<div class="tick"></div>
			</div>

			{#if chosen}
				{@const project = chosen}
				{@const color = paletteColor(project.slug, project.color)}
				{@const goal = targetMinutes(project)}
				{@const spentThisWeek = tracked(project)}
				<div class="card">
					<div class="swatch" style:background={color}>
						<Mark mark={project.mark} color={contrastInk(color)} size={22} />
					</div>
					<div class="detail">
						<div class="line">
							<h2>{project.name}</h2>
							{#if goal > 0}
								<span class="numeric">
									{formatHours(spentThisWeek)} / {formatHours(goal)}
								</span>
							{/if}
						</div>
						{#if goal > 0}
							<div class="target">
								<span style:width="{targetFill(spentThisWeek, goal)}%" style:background={color}
								></span>
							</div>
						{/if}
						{#if current.note}<p class="note">{current.note}</p>{/if}
					</div>
				</div>
			{:else if current.note}
				<p class="note bare">{current.note}</p>
			{/if}
		</div>

		<div class="foot">
			<div class="actions">
				<button class="primary" onclick={stop} disabled={busy}>Stop and log</button>
				<button class="discard" onclick={discard} disabled={busy} aria-label="Discard">×</button>
			</div>
		</div>
	</section>
{:else}
	<!-- 2c: pick a project as a coloured square, then start. -->
	<section class="screen">
		<header class="hero">
			<div class="hero-top">
				<h1>READY<br /><strong>FOR {settings?.focus ?? '25m'}</strong></h1>
				<a class="settings" href="/settings">Settings</a>
			</div>
			<div class="rule"></div>
		</header>

		<div class="grid">
			{#each projects as project (project.slug)}
				{@const color = paletteColor(project.slug, project.color)}
				{@const ink = contrastInk(color)}
				{@const goal = targetMinutes(project)}
				<button
					class="tile"
					style:background={color}
					style:color={ink}
					aria-pressed={chosenProject === project.slug}
					onclick={() => (chosenProject = chosenProject === project.slug ? '' : project.slug)}
				>
					<Mark mark={project.mark} color={ink} size={24} />
					<span class="tile-foot">
						<span class="tile-name">{project.name}</span>
						{#if goal > 0}
							<span class="tile-hours numeric">
								{formatHours(tracked(project))} / {formatHours(goal)}
							</span>
						{/if}
					</span>
				</button>
			{/each}

			<a class="tile new" href="/projects/new">
				<span class="plus">+</span>
				<span class="tile-foot"><span class="tile-name">New<br />project</span></span>
			</a>
		</div>

		<div class="foot">
			<label class="label" for="note">Note</label>
			<input
				id="note"
				class="underlined"
				type="text"
				placeholder="What are you working on?"
				bind:value={note}
			/>

			<button class="start" onclick={startFocus} disabled={busy}>
				<span>Start</span>
				<span class="play"></span>
			</button>
		</div>
	</section>
{/if}

<style>
	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--gap);
		padding: 14px var(--pad) 10px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.middle {
		flex: 1;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 28px;
		padding: 24px var(--pad);
	}

	.foot {
		padding: 12px var(--pad) 16px;
	}

	.logo {
		display: flex;
		gap: 10px;
		align-items: center;
	}

	.logo.small {
		gap: 5px;
	}

	/* ---- 2a: the dial ---------------------------------------------------- */

	.dial {
		position: relative;
		width: min(72vw, 288px);
		aspect-ratio: 1;
		margin: 0 auto;
		border: var(--rule) solid var(--ink);
		border-radius: 50%;
		/* The sweep is one paint rather than an SVG arc: no path arithmetic, and
		   it stays exact at any size. */
		background: conic-gradient(
			var(--red) 0deg calc(var(--spent) * 360deg),
			var(--paper) calc(var(--spent) * 360deg) 360deg
		);
	}

	.face {
		position: absolute;
		inset: 16%;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
		border: var(--rule) solid var(--ink);
		border-radius: 50%;
		background: var(--paper);
	}

	.face strong {
		font-size: clamp(2.6rem, 15vw, 3.625rem);
		font-weight: 500;
		line-height: 1;
		letter-spacing: -0.02em;
	}

	.face span {
		font-size: 0.6875rem;
		font-weight: 500;
		letter-spacing: 0.2em;
		text-transform: uppercase;
	}

	.tick {
		position: absolute;
		left: 50%;
		top: -9px;
		width: 2px;
		height: 18px;
		background: var(--ink);
		transform: translateX(-50%);
	}

	/* ---- 2a: the project card -------------------------------------------- */

	.card {
		display: flex;
		align-items: stretch;
		border: var(--rule) solid var(--ink);
		background: var(--white);
	}

	.swatch {
		flex: none;
		width: 56px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-right: var(--rule) solid var(--ink);
	}

	.detail {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 7px;
		padding: 11px 13px;
	}

	.line {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 10px;
	}

	.line span {
		font-size: 0.75rem;
	}

	.note {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--ink-60);
	}

	.note.bare {
		text-align: center;
	}

	.discard {
		flex: none !important;
		width: 54px;
		background: var(--red);
		color: var(--paper);
		font-size: 1.5rem;
		letter-spacing: 0;
	}

	/* ---- 3a: break ------------------------------------------------------- */

	.break {
		background: var(--yellow);
	}

	.ring {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 4px;
		width: min(68vw, 250px);
		aspect-ratio: 1;
		margin: 0 auto;
		border: var(--rule) solid var(--ink);
		border-radius: 50%;
	}

	.ring strong {
		font-size: clamp(3rem, 17vw, 4.125rem);
		font-weight: 300;
		line-height: 1;
	}

	.ring span {
		font-size: 0.6875rem;
		font-weight: 500;
		letter-spacing: 0.2em;
		text-transform: uppercase;
		color: var(--ink-60);
	}

	.break p {
		max-width: 260px;
		margin: 0 auto;
		text-align: center;
		font-size: 0.875rem;
		color: var(--ink-80);
	}

	/* ---- 3b: session complete -------------------------------------------- */

	.complete {
		padding: 22px var(--pad) 20px;
		border-bottom: var(--rule) solid var(--ink);
		background: var(--red);
		color: var(--paper);
	}

	.complete .eyebrow {
		color: rgba(242, 239, 230, 0.8);
	}

	.headline {
		display: flex;
		align-items: flex-end;
		gap: 12px;
		margin-top: 12px;
	}

	.headline strong {
		font-size: 3.25rem;
		font-weight: 300;
		line-height: 0.9;
	}

	.named {
		display: flex;
		align-items: center;
		gap: 8px;
		padding-bottom: 6px;
		font-size: 1rem;
		font-weight: 600;
		letter-spacing: 0.03em;
	}

	.body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 20px;
		padding: 20px var(--pad) 0;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 9px;
	}

	.milestone {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 13px 14px;
		background: var(--white);
		text-transform: none;
		letter-spacing: 0;
		font-size: 0.875rem;
	}

	.milestone span:first-of-type {
		flex: 1;
		text-align: left;
	}

	.box {
		flex: none;
		width: 24px;
		height: 24px;
		border: var(--rule) solid var(--ink);
	}

	.week {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 0.8125rem;
		color: var(--ink-60);
	}

	.week .hairline {
		flex: 1;
	}

	/* ---- 3k: first run ---------------------------------------------------- */

	.welcome {
		padding: 44px var(--pad) 18px;
	}

	.pitch {
		margin-top: 30px;
	}

	.pitch h1 {
		font-size: 2.875rem;
		font-weight: 300;
		line-height: 0.92;
		letter-spacing: -0.02em;
	}

	.pitch h1 strong {
		font-weight: 600;
	}

	.pitch .rule {
		margin-top: 20px;
	}

	.pitch p {
		max-width: 270px;
		margin: 16px 0 0;
		font-size: 0.9375rem;
		color: var(--ink-80);
	}

	.welcome form {
		display: flex;
		flex-direction: column;
		flex: 1;
		margin-top: 32px;
	}

	.spacer {
		flex: 1;
		min-height: 32px;
	}

	.footnote {
		margin: 13px 0 0;
		text-align: center;
		font-size: 0.78125rem;
		color: var(--ink-45);
	}

	/* ---- 2c: idle home ---------------------------------------------------- */

	.hero {
		padding: var(--gap) var(--pad) 0;
	}

	.hero-top {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--gap);
	}

	.hero h1 {
		font-size: 2.75rem;
		font-weight: 300;
		line-height: 0.88;
		letter-spacing: -0.02em;
	}

	.hero h1 strong {
		font-weight: 600;
	}

	/* Vertical only, and the reason is optical: the 44px tap box is taller than
	   the eyebrow beside it, so centring it in the row sat the label low. */
	.settings {
		flex: none;
		display: flex;
		align-items: center;
		min-height: var(--tap-target);
		padding: 0 0 0 12px;
		margin-top: -6px;
		font-size: 0.6875rem;
		font-weight: 500;
		letter-spacing: 0.16em;
		text-transform: uppercase;
		text-decoration: none;
		color: var(--ink-60);
	}

	/*
	 * Pulled out to the hero's edges so it lines up with the project grid below,
	 * which is full-bleed. Inset by the padding it stopped a little short of the
	 * grid on both sides, which reads worse than either extreme.
	 */
	.hero .rule {
		margin: 14px calc(-1 * var(--pad)) 0;
	}

	/*
	 * The 2px black gap is the shelf's own background showing through, which is
	 * why the tiles carry no borders of their own.
	 *
	 * It wraps rather than being a grid, because a grid reserves the cells it
	 * does not fill and the background shows through those too — an odd number
	 * of projects drew a tile-sized black square at the end of the shelf. Here
	 * the tiles on a short last row grow into the space instead, so the only
	 * black is between them. Their height is fixed, so the rows stay even.
	 */
	.grid {
		display: flex;
		flex-wrap: wrap;
		gap: var(--rule);
		margin-top: 18px;
		background: var(--ink);
		border-top: var(--rule) solid var(--ink);
		border-bottom: var(--rule) solid var(--ink);
	}

	.tile {
		display: flex;
		flex-direction: column;
		justify-content: space-between;
		align-items: flex-start;
		gap: 10px;
		/* Two across: two halves either side of one 2px gap. */
		flex: 1 1 calc(50% - 1px);
		height: 118px;
		padding: 16px 14px;
		border: none;
		text-align: left;
		text-transform: none;
		letter-spacing: 0;
		text-decoration: none;
	}

	.tile[aria-pressed='true'] {
		box-shadow: inset 0 0 0 4px currentColor;
	}

	/*
	 * Always the last tile, and it runs to the end of its row so the shelf never
	 * ends on an empty cell. The 2px gaps are the grid's black background
	 * showing through, so an unfilled cell is not a gap — it is a tile-sized
	 * black square, which is what an odd number of projects used to draw.
	 * Closing the row with the invitation to add one reads better than a hole.
	 */
	.tile.new {
		background: var(--paper);
		color: var(--ink);
	}

	.plus {
		font-size: 1.875rem;
		font-weight: 300;
		line-height: 1;
	}

	.tile-foot {
		display: block;
		min-width: 0;
		max-width: 100%;
	}

	.tile-name {
		display: block;
		font-size: 1rem;
		font-weight: 600;
		letter-spacing: 0.03em;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.tile.new .tile-name {
		font-size: 0.8125rem;
		font-weight: 500;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		white-space: normal;
	}

	.tile-hours {
		display: block;
		margin-top: 5px;
		font-size: 0.71875rem;
		opacity: 0.75;
	}

	.grid + .foot {
		margin-top: auto;
	}

	/* The one control on the screen that has to be unmissable. */
	.start {
		display: flex;
		align-items: center;
		gap: 0;
		width: 100%;
		margin-top: var(--gap);
		padding: 0;
		height: 58px;
		background: var(--yellow);
		font-size: 1.0625rem;
		font-weight: 600;
		letter-spacing: 0.2em;
	}

	.start > span:first-child {
		flex: 1;
	}

	.play {
		flex: none;
		width: 58px;
		height: 100%;
		display: grid;
		place-items: center;
		background: var(--ink);
		border-left: var(--rule) solid var(--ink);
	}

	.play::after {
		content: '';
		width: 0;
		height: 0;
		border-left: 16px solid var(--paper);
		border-top: 10px solid transparent;
		border-bottom: 10px solid transparent;
	}

	/* ---- wide ------------------------------------------------------------ */

	@media (min-width: 700px) {
		/* The sidebar carries it there; two entry points would be one too many. */
		.settings {
			display: none;
		}

		/* On a phone the start button is pushed to the thumb; on anything taller
		   that just opens a void down the middle of the screen. */
		.grid + .foot {
			margin-top: var(--gap);
		}

		.middle {
			gap: 36px;
		}

		/* Nothing on the first-run screen wants the full measure. Its headline
		   grows here rather than at the content breakpoint: the screen is a
		   560px column on any desktop, so it never has two columns to ask
		   about, and its size is the one decision this width already makes. */
		.welcome {
			max-width: 560px;
		}

		.pitch h1 {
			font-size: 3.5rem;
		}

		/*
		 * The cap goes on the screen, not on its middle. Capping `.body` alone
		 * left the red band above it and the action bar below it running the
		 * full measure, so the state had three right edges. `.welcome` is the
		 * model: the state's own class carries the width.
		 */
		.finished {
			max-width: 620px;
		}
	}

	@container screen (min-width: 900px) {
		.dial {
			width: 340px;
		}

		.ring {
			width: 300px;
		}

		/* A phone fits two tiles across; a desktop fits four, and the row of them
		   reads as the shelf of projects the design draws. Four quarters either
		   side of three 2px gaps. */
		.tile {
			flex-basis: calc(25% - 1.5px);
		}

		.hero h1 {
			font-size: 3.25rem;
		}
	}

	@media (hover: hover) {
		.start:hover {
			background: var(--ink);
			color: var(--yellow);
		}

		/* No hover ring on the tiles. `currentColor` there is paper, so it drew a
		   white border — and it was the same ring that means "selected", which
		   made hovering an unchosen project look like choosing it. */
	}
</style>
