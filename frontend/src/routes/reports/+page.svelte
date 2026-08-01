<script lang="ts">
	import { api, type GroupBy, type Report } from '$lib/api';
	import { describe } from '$lib/attempt';
	import { parseMinutes } from '$lib/countdown';
	import {
		dayLabel,
		endOfMonth,
		shiftDays,
		shiftMonths,
		startOfMonth,
		startOfWeek,
		today
	} from '$lib/dates';

	type Span = 'week' | 'month';

	let span = $state<Span>('week');
	let groupBy = $state<GroupBy>('project');
	let anchor = $state(today());
	let report = $state<Report | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(true);

	const from = $derived(span === 'week' ? startOfWeek(anchor) : startOfMonth(anchor));
	const to = $derived(span === 'week' ? shiftDays(startOfWeek(anchor), 6) : endOfMonth(anchor));

	/** Each bucket paired with its size, measured once per report rather than
	    once per bucket per render. */
	const bars = $derived(
		(report?.buckets ?? []).map((bucket) => ({ bucket, minutes: parseMinutes(bucket.tracked) }))
	);

	/** Longest bar in the set, so the bars are relative to the biggest bucket. */
	const peak = $derived(bars.reduce((most, bar) => Math.max(most, bar.minutes), 0));

	const move = (steps: number): void => {
		anchor = span === 'week' ? shiftDays(anchor, steps * 7) : shiftMonths(anchor, steps);
	};

	$effect(() => {
		// Re-runs whenever the range or grouping changes.
		const [start, end, grouping] = [from, to, groupBy];
		error = null;
		api
			.readReport(start, end, grouping)
			.then((result) => {
				report = result;
			})
			.catch((failure: unknown) => {
				error = describe(failure);
			})
			.finally(() => {
				loading = false;
			});
	});
</script>

<header>
	<h1>Reports</h1>
	{#if report}
		<strong>{report.total}</strong>
	{/if}
</header>

<div class="controls">
	<div class="segmented" role="group" aria-label="Range">
		<button class:active={span === 'week'} onclick={() => (span = 'week')}>Week</button>
		<button class:active={span === 'month'} onclick={() => (span = 'month')}>Month</button>
	</div>
	<div class="segmented" role="group" aria-label="Grouping">
		<button class:active={groupBy === 'project'} onclick={() => (groupBy = 'project')}>
			By project
		</button>
		<button class:active={groupBy === 'day'} onclick={() => (groupBy = 'day')}>By day</button>
	</div>
</div>

<nav class="range">
	<button class="quiet" aria-label="Previous range" onclick={() => move(-1)}>‹</button>
	<span>{from} → {to}</span>
	<button class="quiet" aria-label="Next range" onclick={() => move(1)}>›</button>
</nav>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}

{#if loading}
	<p class="muted">Loading…</p>
{:else if report && report.buckets.length === 0}
	<p class="muted">Nothing tracked in this range.</p>
{:else if report}
	<ul>
		{#each bars as { bucket, minutes } (bucket.key ?? '·')}
			<li>
				<div class="line">
					<span class="name">
						{#if bucket.key === null}
							<em>No project</em>
						{:else if report.groupBy === 'day'}
							{dayLabel(bucket.key)}
						{:else}
							{bucket.key}
						{/if}
					</span>
					<span class="value">{bucket.tracked}</span>
				</div>
				<div class="bar" style:width="{peak > 0 ? (minutes / peak) * 100 : 0}%"></div>
			</li>
		{/each}
	</ul>
{/if}

<style>
	header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		margin-bottom: var(--gap);
	}

	header strong {
		font-size: 1.3rem;
		font-variant-numeric: tabular-nums;
	}

	.controls {
		display: flex;
		gap: 8px;
		margin-bottom: 8px;
	}

	.segmented {
		display: flex;
		flex: 1;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		overflow: hidden;
	}

	.segmented button {
		flex: 1;
		border: none;
		border-radius: 0;
		padding: 0 8px;
		font-size: 0.85rem;
		background: var(--surface-raised);
	}

	.segmented button.active {
		background: var(--accent);
		color: var(--accent-contrast);
		font-weight: 600;
	}

	.range {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		margin-bottom: var(--gap);
		color: var(--text-muted);
		font-size: 0.85rem;
		font-variant-numeric: tabular-nums;
	}

	.range button {
		font-size: 1.3rem;
		line-height: 1;
		padding: 0 12px;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.line {
		display: flex;
		justify-content: space-between;
		gap: 8px;
		margin-bottom: 4px;
	}

	.name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.value {
		font-variant-numeric: tabular-nums;
		color: var(--text-muted);
	}

	.bar {
		height: 8px;
		min-width: 2px;
		border-radius: 999px;
		background: var(--accent);
	}
</style>
