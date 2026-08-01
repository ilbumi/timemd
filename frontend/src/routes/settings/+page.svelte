<script lang="ts">
	import { api, type Settings } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { parseMinutes } from '$lib/countdown';
	import {
		enablePush,
		isIos,
		isStandalone,
		isSubscribed,
		isSupported,
		type PushOutcome
	} from '$lib/notifications';

	/** The three lengths, each with the colour the design gives it. */
	const LENGTHS = [
		{ key: 'focus', label: 'Focus', fill: 'var(--red)', ink: 'var(--paper)' },
		{ key: 'shortBreak', label: 'Break', fill: 'var(--yellow)', ink: 'var(--ink)' },
		{ key: 'longBreak', label: 'Long', fill: 'var(--blue)', ink: 'var(--paper)' }
	] as const;

	let settings = $state<Settings | null>(null);
	let error = $state<string | null>(null);
	let subscribed = $state(false);
	let busy = $state(false);
	let outcome = $state<PushOutcome | null>(null);

	// iOS delivers push only to an installed app, so a phone still in Safari
	// needs the install step before the button can do anything useful.
	const mustInstallFirst = $derived(isIos() && !isStandalone());

	const messages: Record<PushOutcome, string> = {
		enabled: 'Notifications are on for this device.',
		denied: 'Permission was refused. Turn notifications back on in your browser settings.',
		unsupported: 'This browser cannot do push notifications.',
		'needs-install':
			'Add timemd to your Home Screen first — iOS only delivers push to installed apps.',
		failed: 'Could not subscribe. Check that the server is reachable and try again.'
	};

	function minutesOf(key: (typeof LENGTHS)[number]['key']): number {
		return settings === null ? 0 : parseMinutes(settings[key]);
	}

	const adjust = (key: (typeof LENGTHS)[number]['key'], by: number): Promise<void> =>
		run(async () => {
			const next = Math.min(120, Math.max(1, minutesOf(key) + by));
			settings = await api.writeSettings({ [key]: `${next}m` });
		});

	async function run(work: () => Promise<void>): Promise<void> {
		busy = true;
		error = await attempt(work);
		busy = false;
	}

	async function turnOn(): Promise<void> {
		busy = true;
		outcome = await enablePush();
		subscribed = outcome === 'enabled';
		busy = false;
	}

	$effect(() => {
		void run(async () => {
			settings = await api.readSettings();
		});

		isSubscribed()
			.then((already) => {
				subscribed = already;
			})
			.catch(() => {
				// Not being able to tell is the same as not subscribed here.
			});
	});
</script>

<section class="screen">
	<header class="topbar">
		<h1>SETTINGS</h1>
		<a class="back" href="/">Timer</a>
	</header>

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<div class="body">
		<div class="field">
			<span class="label">Durations</span>
			<div class="lengths">
				{#each LENGTHS as length (length.key)}
					<div class="length" style:background={length.fill} style:color={length.ink}>
						<div class="stepper">
							<button
								aria-label="Shorten {length.label}"
								disabled={busy || settings === null}
								onclick={() => adjust(length.key, -5)}>−</button
							>
							<strong class="numeric">{minutesOf(length.key)}</strong>
							<button
								aria-label="Lengthen {length.label}"
								disabled={busy || settings === null}
								onclick={() => adjust(length.key, 5)}>+</button
							>
						</div>
						<span>{length.label}</span>
					</div>
				{/each}
			</div>
			{#if settings}
				<p class="meta">
					A long break every {settings.longBreakEvery} sessions. Reminders lead by {settings.remindBefore}
					unless a block says otherwise.
				</p>
			{/if}
		</div>

		<div class="field">
			<span class="label">Notifications</span>

			{#if mustInstallFirst}
				<div class="banner">
					<span class="chip"></span>
					<p>
						<strong>Add to Home Screen first.</strong> iOS only delivers notifications to installed
						apps. Tap Share, then <em>Add to Home Screen</em>, and open timemd from there.
					</p>
				</div>
			{:else if !isSupported()}
				<p class="meta">This browser cannot do push notifications.</p>
			{/if}

			<button
				class="primary wide"
				onclick={turnOn}
				disabled={busy || subscribed || mustInstallFirst}
			>
				{subscribed ? 'Notifications are on' : 'Turn on notifications'}
			</button>

			{#if outcome}
				<p class="meta" class:bad={outcome !== 'enabled'} role="status">{messages[outcome]}</p>
			{/if}
		</div>

		<div class="field">
			<span class="label">Data</span>
			<p class="meta">
				Everything lives in markdown under the server's data directory — projects in
				<code>projects/</code>, tracked time in <code>days/</code>, repeating blocks in
				<code>schedule/recurring.md</code>. Edit them by hand or with an agent; the app reads them
				back on the next request.
			</p>
			{#if settings}
				<p class="meta">Times are wall-clock in <code>{settings.timezone}</code>.</p>
			{/if}
		</div>
	</div>
</section>

<style>
	.screen {
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}

	.back {
		display: flex;
		align-items: center;
		min-height: var(--tap-target);
		padding-left: 12px;
		font-size: 0.6875rem;
		font-weight: 500;
		letter-spacing: 0.16em;
		text-transform: uppercase;
		text-decoration: none;
		color: var(--ink-60);
	}

	.body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 22px;
		padding: 20px var(--pad) 24px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.lengths {
		display: flex;
		border: var(--rule) solid var(--ink);
	}

	.length {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 5px;
		padding: 10px 0 12px;
	}

	.length + .length {
		border-left: var(--rule) solid var(--ink);
	}

	.stepper {
		display: flex;
		align-items: center;
		gap: 2px;
	}

	/*
	 * The glyphs stay 30×34 because that is what the design draws either side of
	 * the number; the thumb gets its 44px from an invisible overlay instead, the
	 * same way the pattern editor's switch does. The overlays reach 7px inward
	 * over the number, which is not a target, and so never over each other.
	 */
	.stepper button {
		position: relative;
		min-height: 34px;
		width: 30px;
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		font-size: 1.125rem;
		font-weight: 300;
		opacity: 0.75;
	}

	.stepper button::after {
		content: '';
		position: absolute;
		inset: -5px -7px;
	}

	.stepper strong {
		min-width: 34px;
		text-align: center;
		font-size: 1.5rem;
		font-weight: 500;
		line-height: 1;
	}

	.length > span {
		font-size: 0.65625rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		opacity: 0.8;
	}

	.banner {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 13px 15px;
		background: var(--ink);
		color: var(--paper);
	}

	.banner p {
		margin: 0;
		font-size: 0.78125rem;
		line-height: 1.4;
	}

	.chip {
		flex: none;
		width: 18px;
		height: 18px;
		margin-top: 2px;
		background: var(--yellow);
	}

	.wide {
		width: 100%;
	}

	.bad {
		color: var(--red);
	}

	/* ---- wide ------------------------------------------------------------ */

	@media (min-width: 700px) {
		/* The sidebar is the way back; a second link would be one too many. */
		.back {
			display: none;
		}

		/* The screen narrows, not just its middle: capping the body alone left
		   the header's rule hanging out over empty paper. */
		.screen {
			max-width: 560px;
		}
	}
</style>
