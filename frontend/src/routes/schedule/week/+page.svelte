<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import ScheduleTabs from '$lib/ScheduleTabs.svelte';
	import { api, type Occurrence } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { formatHours } from '$lib/countdown';
	import { isoWeek, minutesOfDay, shiftDays, startOfWeek, today } from '$lib/dates';
	import { lookOf, looksFrom, type Look } from '$lib/look';

	const DAY_LETTERS = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];
	const DEFAULT_FROM = 8 * 60;
	const DEFAULT_TO = 22 * 60;
	const NOW_MS = 60_000;

	let anchor = $state(today());
	let occurrences = $state<Occurrence[]>([]);
	let looks = $state<Record<string, Look>>({});
	let error = $state<string | null>(null);
	let loading = $state(true);
	let nowMinutes = $state(currentMinutes());

	function currentMinutes(): number {
		const now = new Date();
		return now.getHours() * 60 + now.getMinutes();
	}

	const monday = $derived(startOfWeek(anchor));
	const dates = $derived(Array.from({ length: 7 }, (_, offset) => shiftDays(monday, offset)));

	const totalMinutes = $derived(
		occurrences.reduce(
			(total, block) => total + (minutesOfDay(block.end) - minutesOfDay(block.start)),
			0
		)
	);

	/** Every project that actually appears this week, for the legend. */
	const legend = $derived.by(() => {
		const slugs = [...new Set(occurrences.map((block) => block.project))];
		return slugs.map((slug) => lookOf(looks, slug));
	});

	const span = $derived.by(() => {
		const starts = occurrences.map((block) => minutesOfDay(block.start));
		const ends = occurrences.map((block) => minutesOfDay(block.end));
		const from = Math.min(DEFAULT_FROM, ...starts);
		const to = Math.max(DEFAULT_TO, ...ends);
		return { from, to: Math.max(to, from + 60) };
	});

	const hours = $derived.by(() => {
		const marks: number[] = [];
		for (let hour = Math.ceil(span.from / 60); hour * 60 <= span.to; hour += 3) {
			marks.push(hour);
		}
		return marks;
	});

	function offset(minutes: number): number {
		return ((minutes - span.from) / (span.to - span.from)) * 100;
	}

	function blocksOn(date: string): Occurrence[] {
		return occurrences.filter((block) => block.date === date);
	}

	async function load(): Promise<void> {
		error = await attempt(async () => {
			occurrences = await api.readSchedule(monday, shiftDays(monday, 6));
		});
		loading = false;
	}

	$effect(() => {
		void monday;
		void load();
	});

	$effect(() => {
		api
			.listProjects()
			.then((all) => {
				looks = looksFrom(all);
			})
			.catch(() => {
				// Blocks fall back to a derived colour without it.
			});

		const tick = setInterval(() => {
			nowMinutes = currentMinutes();
		}, NOW_MS);
		return () => clearInterval(tick);
	});
</script>

<section class="screen">
	<header class="head">
		<div class="head-top">
			<button
				class="quiet"
				aria-label="Previous week"
				onclick={() => (anchor = shiftDays(anchor, -7))}>‹</button
			>
			<h1>WEEK<br /><span class="light">{isoWeek(monday)}</span></h1>
			<div class="totals meta">{formatHours(totalMinutes)}<br />planned</div>
			<button class="quiet" aria-label="Next week" onclick={() => (anchor = shiftDays(anchor, 7))}>
				›
			</button>
		</div>
		<ScheduleTabs />
	</header>

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<div class="raster">
		<div class="gutter">
			{#each hours as hour (hour)}
				<span style:top="{offset(hour * 60)}%">{hour.toString().padStart(2, '0')}</span>
			{/each}
		</div>

		<div class="columns">
			{#each dates as date, index (date)}
				{@const isToday = date === today()}
				<div class="column" class:today={isToday}>
					<div class="stack">
						{#each blocksOn(date) as block, position (`${block.start}-${block.title}-${position}`)}
							{@const look = lookOf(looks, block.project)}
							<span
								class="chip"
								style:top="{offset(minutesOfDay(block.start))}%"
								style:height="{offset(minutesOfDay(block.end)) -
									offset(minutesOfDay(block.start))}%"
								style:background={look.color}
								title="{block.title || look.name} · {block.start}–{block.end}"
							></span>
						{/each}

						{#if isToday && nowMinutes >= span.from && nowMinutes <= span.to}
							<span class="now" style:top="{offset(nowMinutes)}%" aria-hidden="true"></span>
						{/if}
					</div>
					<div class="letter" class:weekend={index > 4}>{DAY_LETTERS[index]}</div>
				</div>
			{/each}
		</div>
	</div>

	{#if loading}
		<p class="empty">Loading…</p>
	{:else if occurrences.length === 0}
		<p class="empty">Nothing planned this week.</p>
	{/if}

	<div class="foot">
		<ul class="key">
			{#each legend as look (look.name)}
				<li><Mark mark={look.mark} color={look.color} size={12} />{look.name}</li>
			{/each}
		</ul>

		<div class="actions">
			<a class="button" href="/schedule/pattern">Pattern</a>
			<a class="button fill" href="/schedule">+ Block</a>
		</div>
	</div>
</section>

<style>
	.screen {
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}

	.head {
		padding: 14px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	.head-top {
		display: flex;
		align-items: flex-end;
		gap: 8px;
		margin-bottom: 14px;
	}

	.head-top h1 {
		flex: 1;
	}

	.light {
		font-weight: 300;
	}

	.totals {
		text-align: right;
		text-transform: uppercase;
	}

	.head-top button {
		font-size: 1.5rem;
		line-height: 1;
		min-height: 0;
		align-self: center;
	}

	.raster {
		flex: 1;
		display: flex;
		min-height: 300px;
		padding: 10px var(--pad) 0;
	}

	.gutter {
		position: relative;
		flex: none;
		width: 26px;
		margin-bottom: 22px;
	}

	.gutter span {
		position: absolute;
		left: 0;
		transform: translateY(-50%);
		font-size: 0.625rem;
		letter-spacing: 0.06em;
	}

	.columns {
		flex: 1;
		display: flex;
		gap: 3px;
	}

	.column {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.stack {
		position: relative;
		flex: 1;
		border-left: 1px solid rgba(17, 17, 17, 0.18);
	}

	/* Today is the only column with a field and hard edges — the week's "you are
	   here" without adding a colour the palette does not have. */
	.column.today .stack {
		border-left: var(--rule) solid var(--ink);
		border-right: var(--rule) solid var(--ink);
		background: rgba(233, 184, 58, 0.16);
	}

	.chip {
		position: absolute;
		left: 0;
		right: 0;
		min-height: 3px;
	}

	.now {
		position: absolute;
		left: -7px;
		right: -7px;
		height: 2px;
		background: var(--ink);
	}

	.letter {
		height: 22px;
		display: grid;
		place-items: center;
		font-size: 0.6875rem;
		font-weight: 600;
	}

	.letter.weekend {
		font-weight: 400;
		color: var(--ink-45);
	}

	.foot {
		padding: 13px var(--pad) 16px;
		border-top: var(--rule) solid var(--ink);
	}

	.key {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 16px;
		list-style: none;
		margin: 0 0 11px;
		padding: 0;
	}

	.key li {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 0.71875rem;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}

	/* Anchors, because they navigate — dressed as the buttons beside them. */
	.button {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 48px;
		font-size: 0.8125rem;
		font-weight: 500;
		letter-spacing: 0.16em;
		text-transform: uppercase;
		text-decoration: none;
	}

	.button + .button {
		border-left: var(--rule) solid var(--ink);
	}

	.fill {
		background: var(--red);
		color: var(--paper);
		font-weight: 600;
	}
</style>
