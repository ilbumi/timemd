<script lang="ts">
	import { api, type Project, type SessionKind, type TimerState } from '$lib/api';
	import { describe } from '$lib/attempt';
	import { Countdown, formatClock, progress } from '$lib/countdown';

	/** How often to re-ask the server while the screen is visible. */
	const POLL_MS = 20_000;
	/** Circumference of the dial at r=54, for the sweep. */
	const DIAL_LENGTH = 339.29;
	/** How often to redraw the countdown between polls. */
	const TICK_MS = 250;

	const countdown = new Countdown();

	let timer = $state<TimerState | null>(null);
	let projects = $state<Project[]>([]);
	let error = $state<string | null>(null);
	let busy = $state(false);
	let seconds = $state(0);
	let chosenProject = $state('');
	let note = $state('');

	const running = $derived(timer?.active ?? null);
	const spent = $derived(progress(seconds, running?.durationSeconds ?? 0));
	const kindLabel = $derived(
		running
			? { focus: 'Focus', short_break: 'Short break', long_break: 'Long break' }[running.kind]
			: 'Ready'
	);

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
		timer = next;
		countdown.sync(next.active?.remainingSeconds ?? null, performance.now());
		seconds = countdown.remaining(performance.now());
		if (next.active) {
			chosenProject = next.active.project ?? '';
			note = next.active.note;
		}
	}

	const refresh = (): Promise<void> => run(() => api.readTimer());

	const startFocus = (): Promise<void> =>
		run(() =>
			api.startSession({ kind: 'focus', project: chosenProject || null, note: note.trim() })
		);

	const startBreak = (kind: SessionKind): Promise<void> => run(() => api.startSession({ kind }));

	const stop = (): Promise<void> => run(() => api.stopSession());

	const cancel = (): Promise<void> => run(() => api.cancelSession());

	$effect(() => {
		void refresh();
		api
			.listActiveProjects()
			.then((active) => {
				projects = active;
			})
			.catch(() => {
				// The timer is usable without the project list, so a failure here
				// should not take the screen down with it.
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

<header>
	<h1>Timer</h1>
	{#if timer}
		<p class="muted">{timer.completedToday} today · {timer.trackedToday} tracked</p>
	{/if}
</header>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}

<div class="dial" class:idle={!running}>
	<svg viewBox="0 0 120 120" aria-hidden="true">
		<circle class="track" cx="60" cy="60" r="54" />
		<circle
			class="sweep"
			cx="60"
			cy="60"
			r="54"
			stroke-dasharray={DIAL_LENGTH}
			stroke-dashoffset={DIAL_LENGTH * (1 - spent)}
		/>
	</svg>
	<div class="readout">
		<strong aria-live="polite">{formatClock(seconds)}</strong>
		<span>{kindLabel}</span>
	</div>
</div>

{#if running}
	<p class="now">
		{#if running.project}<span class="tag">{running.project}</span>{/if}
		{running.note}
	</p>

	<div class="actions">
		<button class="primary" onclick={stop} disabled={busy}>Stop and log</button>
		<button class="danger" onclick={cancel} disabled={busy}>Discard</button>
	</div>
{:else}
	<form
		onsubmit={(event) => {
			event.preventDefault();
			void startFocus();
		}}
	>
		<label for="project">Project</label>
		<select id="project" bind:value={chosenProject}>
			<option value="">No project</option>
			{#each projects as project (project.slug)}
				<option value={project.slug}>{project.name}</option>
			{/each}
		</select>

		<label for="note">Note</label>
		<input id="note" type="text" placeholder="What are you working on?" bind:value={note} />

		<div class="actions">
			<button class="primary" type="submit" disabled={busy}>Start focus</button>
		</div>
	</form>

	{#if timer}
		<!-- Bound so the handlers close over a non-null value: the `{#if}` narrows
		     `timer` here, but not inside a callback that runs later. -->
		{@const current = timer}
		<div class="actions breaks">
			<button onclick={() => startBreak('short_break')} disabled={busy}>Short break</button>
			<button onclick={() => startBreak(current.nextBreakKind)} disabled={busy}>
				Suggested · {current.nextBreak}
			</button>
		</div>
	{/if}
{/if}

<style>
	header {
		margin-bottom: var(--gap);
	}

	.dial {
		position: relative;
		width: min(70vw, 260px);
		margin: 8px auto var(--gap);
		aspect-ratio: 1;
	}

	svg {
		width: 100%;
		height: 100%;
		transform: rotate(-90deg);
	}

	circle {
		fill: none;
		stroke-width: 8;
		stroke-linecap: round;
	}

	.track {
		stroke: var(--surface-sunken);
	}

	.sweep {
		stroke: var(--accent);
		transition: stroke-dashoffset 0.25s linear;
	}

	.idle .sweep {
		stroke: transparent;
	}

	.readout {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
	}

	.readout strong {
		font-size: 2.6rem;
		font-variant-numeric: tabular-nums;
		font-weight: 600;
	}

	.readout span {
		color: var(--text-muted);
		font-size: 0.85rem;
	}

	.now {
		text-align: center;
		margin: 0 0 var(--gap);
	}

	.tag {
		display: inline-block;
		padding: 2px 8px;
		margin-right: 6px;
		border-radius: 999px;
		background: var(--surface-sunken);
		font-size: 0.8rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	label {
		margin-top: 8px;
	}

	.actions {
		display: flex;
		gap: 8px;
		margin-top: var(--gap);
	}

	.actions button {
		flex: 1;
	}

	.breaks {
		margin-top: 8px;
	}
</style>
