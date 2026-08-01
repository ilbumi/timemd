<script lang="ts">
	import Mark from '$lib/Mark.svelte';
	import PeriodHeader from '$lib/PeriodHeader.svelte';
	import { api, type Occurrence } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { formatHours } from '$lib/countdown';
	import { isoWeek, minutesOfDay, shiftDays, startOfWeek, today, weekDates } from '$lib/dates';
	import { lookOf, readLooks, type Look } from '$lib/look';
	import { hourMarks, minutesNow, offsetIn, placeIn, spanOf } from '$lib/timeline';

	const DAY_LETTERS = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];
	const DEFAULT_FROM = 8 * 60;
	const DEFAULT_TO = 22 * 60;
	/** Every third hour: seven columns leave less room for the gutter than one. */
	const HOUR_STEP = 3;
	const NOW_MS = 60_000;

	/** Fixed for the life of the screen — the week does not redraw at midnight,
	    and calling `today()` per column allocated a Date seven times a render. */
	const currentDay = today();

	let anchor = $state(today());
	let occurrences = $state<Occurrence[]>([]);
	let looks = $state<Record<string, Look>>({});
	let error = $state<string | null>(null);
	let loading = $state(true);
	let nowMinutes = $state(minutesNow());

	const monday = $derived(startOfWeek(anchor));
	const dates = $derived(weekDates(monday));

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

	const span = $derived(spanOf(occurrences, DEFAULT_FROM, DEFAULT_TO));
	const hours = $derived(hourMarks(span, HOUR_STEP));

	/** Grouped once rather than filtering the whole week per column. */
	const byDate = $derived.by(() => {
		const columns = new Map<string, Occurrence[]>();
		for (const block of occurrences) {
			const column = columns.get(block.date);
			if (column === undefined) {
				columns.set(block.date, [block]);
			} else {
				column.push(block);
			}
		}
		return columns;
	});

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
		void readLooks().then((loaded) => {
			looks = loaded.looks;
		});

		const tick = setInterval(() => {
			nowMinutes = minutesNow();
		}, NOW_MS);
		return () => clearInterval(tick);
	});
</script>

<section class="screen">
	<PeriodHeader
		unit="week"
		total="{formatHours(totalMinutes)} planned"
		onPrevious={() => (anchor = shiftDays(anchor, -7))}
		onNext={() => (anchor = shiftDays(anchor, 7))}
	>
		{#snippet title()}
			WEEK<br /><span class="light">{isoWeek(monday)}</span>
		{/snippet}
	</PeriodHeader>

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<div class="raster">
		<div class="gutter">
			{#each hours as hour (hour)}
				<span style:top="{offsetIn(span, hour * 60)}%">{hour.toString().padStart(2, '0')}</span>
			{/each}
		</div>

		<div class="columns">
			{#each dates as date, index (date)}
				{@const isToday = date === currentDay}
				<div class="column" class:today={isToday}>
					<div class="stack">
						{#each byDate.get(date) ?? [] as block, position (`${block.start}-${block.title}-${position}`)}
							{@const look = lookOf(looks, block.project)}
							{@const place = placeIn(span, block)}
							<span
								class="chip"
								style:top="{place.top}%"
								style:height="{place.height}%"
								style:background={look.color}
								title="{block.title || look.name} · {block.start}–{block.end}"
							></span>
						{/each}

						{#if isToday && nowMinutes >= span.from && nowMinutes <= span.to}
							<span class="now" style:top="{offsetIn(span, nowMinutes)}%" aria-hidden="true"></span>
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
	.light {
		font-weight: 300;
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
