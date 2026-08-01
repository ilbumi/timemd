<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import { api, type Project } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { formatHours } from '$lib/countdown';
	import { contrastInk, paletteColor } from '$lib/palette';
	import {
		readLifetimeTotals,
		readWeekTotals,
		targetFill,
		targetMinutes,
		totalsFor,
		type Totals
	} from '$lib/totals';

	let projects = $state<Project[]>([]);
	let week = $state<Record<string, Totals>>({});
	let lifetime = $state<Record<string, Totals>>({});
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showArchived = $state(false);

	const active = $derived(projects.filter((project) => project.status === 'active'));
	const archived = $derived(projects.filter((project) => project.status === 'archived'));

	/** The line under the bar. Deliberately about milestones rather than lifetime
	    hours: the year-long total costs a 366-day scan, and only the archived
	    rows — which are collapsed until asked for — actually show it. */
	function subtitle(project: Project): string {
		if (project.milestones.length === 0) return 'no milestones yet';
		const done = project.milestones.filter((milestone) => milestone.done).length;
		return `${done} of ${project.milestones.length} milestones`;
	}

	async function load(): Promise<void> {
		error = await attempt(async () => {
			projects = await api.listProjects();
		});
		loading = false;
	}

	const restore = (project: Project): Promise<void> =>
		run(async () => {
			const updated = await api.updateProject(project.slug, { status: 'active' });
			projects = projects.map((candidate) =>
				candidate.slug === updated.slug ? updated : candidate
			);
		});

	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(work);
	}

	$effect(() => {
		void load();
		void readWeekTotals().then((rows) => {
			week = rows;
		});
	});

	$effect(() => {
		// A year of day files is the most expensive read the app makes, and it is
		// only ever rendered by the archived rows — so it waits until they open.
		if (showArchived) void readLifetimeTotals().then((rows) => (lifetime = rows));
	});
</script>

<section class="screen">
	<header class="topbar">
		<h1>PROJECTS</h1>
		<span class="meta">{active.length} active</span>
	</header>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	<div class="list">
		{#if loading}
			<p class="empty">Loading…</p>
		{:else if projects.length === 0}
			<p class="empty">
				Nothing here yet. Add one below, or drop a markdown file in <code>projects/</code>.
			</p>
		{/if}

		{#each active as project (project.slug)}
			{@const color = paletteColor(project.slug, project.color)}
			{@const goal = targetMinutes(project)}
			{@const tracked = totalsFor(week, project.slug).tracked}
			<a class="row" href="/projects/{project.slug}">
				<span class="swatch" style:background={color}>
					<Mark mark={project.mark} color={contrastInk(color)} size={24} />
				</span>
				<span class="detail">
					<span class="line">
						<span class="name">{project.name}</span>
						{#if goal > 0}
							<span class="numeric hours">{formatHours(tracked)} / {formatHours(goal)}</span>
						{/if}
					</span>
					{#if goal > 0}
						<span class="target"
							><span style:width="{targetFill(tracked, goal)}%" style:background={color}
							></span></span
						>
					{/if}
					<span class="subtitle">{subtitle(project)}</span>
				</span>
			</a>
		{/each}

		{#if archived.length > 0}
			<button
				class="archived-toggle"
				aria-expanded={showArchived}
				onclick={() => (showArchived = !showArchived)}
			>
				<span class="hairline"></span>
				<span>Archived · {archived.length}</span>
				<span class="chevron">{showArchived ? '⌃' : '⌄'}</span>
			</button>

			{#if showArchived}
				{#each archived as project (project.slug)}
					{@const logged = totalsFor(lifetime, project.slug)}
					<div class="row archived">
						<a class="swatch outline" href="/projects/{project.slug}" aria-label={project.name}>
							<Mark mark={project.mark} color="var(--ink-45)" size={22} outline />
						</a>
						<a class="detail" href="/projects/{project.slug}">
							<span class="name faded">{project.name}</span>
							<span class="subtitle">
								{logged.tracked === 0 ? 'nothing logged' : `${formatHours(logged.tracked)} logged`}
								· archived
							</span>
						</a>
						<button class="restore" onclick={() => restore(project)}>Restore</button>
					</div>
				{/each}

				<p class="empty">
					Archived projects keep their history but leave the picker, the schedule and your weekly
					targets.
				</p>
			{/if}
		{/if}
	</div>

	<div class="foot">
		<a class="new" href="/projects/new">+ New project</a>
	</div>
</section>

<style>
	.list {
		flex: 1;
	}

	/* A row is the full width of the screen: the colour field runs to the edge,
	   which is what makes the list read as a stack of blocks rather than cards. */
	.row {
		display: flex;
		align-items: stretch;
		border-bottom: var(--rule) solid var(--ink);
		text-decoration: none;
		color: inherit;
	}

	.row.archived {
		border-bottom: 1px solid var(--ink-15);
	}

	.swatch {
		flex: none;
		width: 62px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.swatch.outline {
		border-right: 1px solid var(--ink-15);
	}

	.detail {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 9px;
		padding: 14px 15px;
		text-decoration: none;
		color: inherit;
	}

	.line {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 10px;
	}

	.name {
		font-size: 1.0625rem;
		font-weight: 600;
		letter-spacing: 0.02em;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.name.faded {
		font-size: 0.96875rem;
		font-weight: 500;
		color: var(--ink-60);
	}

	.hours {
		flex: none;
		font-size: 0.78125rem;
	}

	.subtitle {
		font-size: 0.75rem;
		color: var(--ink-45);
	}

	.archived-toggle {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: var(--tap-target);
		padding: 14px var(--pad);
		border: none;
		background: var(--paper-sunk);
		font-size: 0.6875rem;
		letter-spacing: 0.16em;
		color: var(--ink-60);
	}

	.archived-toggle .hairline {
		flex: 1;
	}

	.chevron {
		letter-spacing: 0;
	}

	.restore {
		flex: none;
		align-self: center;
		margin-right: 15px;
		min-height: 34px;
		padding: 0 9px;
		border-width: 1px;
		font-size: 0.6875rem;
		letter-spacing: 0.12em;
	}

	.foot {
		padding: var(--gap) var(--pad) 16px;
	}

	/* An anchor, not a button, because it navigates — styled as the design's
	   yellow bar so it still reads as the screen's one action. */
	.new {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 54px;
		border: var(--rule) solid var(--ink);
		background: var(--yellow);
		font-size: 0.875rem;
		font-weight: 600;
		letter-spacing: 0.18em;
		text-transform: uppercase;
		text-decoration: none;
	}

	/* ---- wide ------------------------------------------------------------ */

	@media (min-width: 700px) {
		/*
		 * Still one column of full-bleed rows — the row is the design's unit and
		 * two abreast breaks the banding the archived section depends on. What
		 * changes is that the row now has room to breathe.
		 */
		.detail {
			padding: 18px var(--pad);
		}

		.swatch {
			width: 76px;
		}

		/* Same reason as the timer's start button: nothing needs pinning to the
		   bottom of a tall screen. */
		.list {
			flex: none;
		}

		/*
		 * A target bar nine hundred pixels long stops reading as a quantity — but
		 * it is the bar that wants the limit, not the row. Capping the row left
		 * its rule running 200px past everything inside it and the hours landing
		 * nowhere near the edge they are aligned to. Same call the log makes: the
		 * bands stay full width, and only the part that needs a short line gets
		 * one.
		 */
		.target {
			max-width: 620px;
		}

		/* The one action does not need nine hundred pixels of yellow. */
		.new {
			max-width: 320px;
		}
	}

	@media (hover: hover) {
		.row:hover {
			background: var(--paper-sunk);
		}

		.new:hover {
			background: var(--ink);
			color: var(--yellow);
		}
	}
</style>
