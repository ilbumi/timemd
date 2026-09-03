<script lang="ts">
	import { api, type Ntfy, type NtfyTest, type Settings } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { parseMinutes } from '$lib/countdown';
	import {
		disablePush,
		enablePush,
		isIos,
		isStandalone,
		isSubscribed,
		isSupported,
		type PushOutcome
	} from '$lib/notifications';

	/** The three lengths, each with the colour the design gives it. */
	const LENGTHS = [
		{ key: 'focus', label: 'Focus', fill: 'var(--red)' },
		{ key: 'shortBreak', label: 'Break', fill: 'var(--yellow)' },
		{ key: 'longBreak', label: 'Long', fill: 'var(--blue)' }
	] as const;

	let settings = $state<Settings | null>(null);
	let error = $state<string | null>(null);
	let subscribed = $state(false);
	let busy = $state(false);
	let outcome = $state<PushOutcome | null>(null);

	let ntfy = $state<Ntfy | null>(null);
	// The form's own copy: typing must not fight the last server answer, and the
	// token has no server value to hold — it is write-only.
	let topic = $state('');
	let server = $state('');
	let token = $state('');
	let appUrl = $state('');
	let tested = $state<NtfyTest | null>(null);

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

	/*
	 * A test send proves the server and the token, and cannot prove the topic:
	 * ntfy answers 200 for any name it is given. Saying so is the whole reason
	 * the message is worth showing — "delivered" on its own would read as a
	 * guarantee it is not.
	 */
	const ntfyMessages: Record<NtfyTest, string> = {
		delivered:
			'Sent a test notification. If it does not arrive, check the topic — ntfy accepts any name.',
		rejected: 'The server refused it. An access-controlled topic needs a token.',
		unreachable: 'Could not reach that server.'
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

	/**
	 * One button, both ways. The wrapper existed and had no caller: turning
	 * notifications on was permanent from here, short of clearing site data.
	 *
	 * Through `run` like every other call on this screen. Turning them off
	 * reaches the API, and the API throws on any non-2xx; a throw that skipped
	 * `busy = false` left the button disabled for good with nothing on screen
	 * to say why, which is the one state a user cannot retry out of.
	 */
	const togglePush = (): Promise<void> =>
		run(async () => {
			if (subscribed) {
				await disablePush();
				subscribed = false;
				outcome = null;
			} else {
				outcome = await enablePush();
				subscribed = outcome === 'enabled';
			}
		});

	/** Adopts a server answer, including the fields the form does not hold. */
	function adopt(next: Ntfy): void {
		ntfy = next;
		topic = next.topic ?? '';
		// The public default is not a saved choice. Showing it as a filled value
		// made an empty config look live, and the topic placeholder beside it
		// looked like a topic someone could already be subscribed to.
		server = next.topic !== null || next.server !== 'https://ntfy.sh' ? next.server : '';
		appUrl = next.appUrl ?? '';
		tested = next.test;
		// Never refilled from the server, which does not send it back. Clearing
		// it keeps the box from looking like it holds the stored token.
		token = '';
	}

	/**
	 * Sends null rather than an empty string for a field left blank: the API
	 * reads an absent key as "leave it alone", so `undefined` here would make
	 * clearing a field a silent no-op.
	 *
	 * The token is sent only when something was typed. Retyping it is not a new
	 * destination, and an empty box means "leave it", not "clear it" — there is
	 * a separate way to turn the channel off.
	 */
	const saveNtfy = (): Promise<void> =>
		run(async () => {
			adopt(
				await api.writeNtfy({
					server: server.trim(),
					topic: topic.trim() || null,
					appUrl: appUrl.trim() || null,
					...(token.trim() ? { token: token.trim() } : {})
				})
			);
		});

	const turnNtfyOff = (): Promise<void> =>
		run(async () => {
			adopt(await api.writeNtfy({ topic: null }));
		});

	$effect(() => {
		void run(async () => {
			// Two independent reads: sequential would make the screen wait out
			// both round trips, which on a phone off the LAN is the difference
			// a user actually feels.
			const [durations, channel] = await Promise.all([api.readSettings(), api.readNtfy()]);
			settings = durations;
			adopt(channel);
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
					<div class="length">
						<span class="length-bar" style:background={length.fill}></span>
						<div class="stepper">
							<button
								aria-label="Shorten {length.label} by 5 minutes"
								disabled={busy || settings === null}
								onclick={() => adjust(length.key, -5)}>−5</button
							>
							<strong class="numeric">{minutesOf(length.key)}</strong>
							<button
								aria-label="Lengthen {length.label} by 5 minutes"
								disabled={busy || settings === null}
								onclick={() => adjust(length.key, 5)}>+5</button
							>
						</div>
						<span>{length.label}</span>
					</div>
				{/each}
			</div>
			{#if settings}
				<p class="meta">
					Minutes, in steps of 5. A long break every {settings.longBreakEvery} sessions. Reminders
					lead by {settings.remindBefore} unless a block says otherwise.
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

			<button class="primary wide" onclick={togglePush} disabled={busy || mustInstallFirst}>
				{subscribed ? 'Turn off notifications' : 'Turn on notifications'}
			</button>

			{#if outcome}
				<p class="meta" class:bad={outcome !== 'enabled'} role="status">{messages[outcome]}</p>
			{/if}
		</div>

		<div class="field">
			<span class="label">Notifications on a phone</span>
			<p class="meta">
				Install the ntfy app, subscribe to a topic, and put the same topic here. It works where a
				browser will not wake a service worker. Anyone who knows the topic can read your
				notifications, so pick a name nobody would guess.
			</p>

			<div class="stack">
				<label class="entry">
					<span>Topic</span>
					<input
						bind:value={topic}
						disabled={busy}
						placeholder="a name nobody would guess"
						autocomplete="off"
					/>
				</label>
				<label class="entry">
					<span>Server</span>
					<input bind:value={server} disabled={busy} placeholder="https://ntfy.sh" />
				</label>
				<label class="entry">
					<span>Token</span>
					<input
						bind:value={token}
						disabled={busy}
						type="password"
						placeholder={ntfy?.hasToken ? 'Set — type to replace' : 'only for a private topic'}
					/>
				</label>
				<label class="entry">
					<span>App URL</span>
					<input
						bind:value={appUrl}
						disabled={busy}
						placeholder="the URL of this app"
						autocomplete="off"
					/>
				</label>
			</div>

			<button class="primary wide" onclick={saveNtfy} disabled={busy}>Save</button>

			{#if ntfy?.topic}
				<button class="wide" onclick={turnNtfyOff} disabled={busy}>Turn off ntfy</button>
				<p class="meta">Subscribe in the app to <code>{ntfy.subscribeUrl}</code>.</p>
			{/if}

			{#if tested}
				<p class="meta" class:bad={tested !== 'delivered'} role="status">{ntfyMessages[tested]}</p>
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
	/*
	 * The label is text, so it sits on the title's baseline. It used to be a
	 * 44px box with the text centred inside it, which put it most of the way up
	 * the title and made the bar 74px tall to fit. The thumb gets its 44px from
	 * an invisible overlay instead, the way the steppers below do.
	 */
	.back {
		position: relative;
		padding-left: 12px;
		font-size: 0.6875rem;
		font-weight: 500;
		letter-spacing: 0.16em;
		text-transform: uppercase;
		text-decoration: none;
		color: var(--ink-60);
	}

	.back::after {
		content: '';
		position: absolute;
		inset: -14px 0;
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
		gap: 6px;
		padding: 0 0 12px;
		background: var(--paper);
		color: var(--ink);
	}

	.length + .length {
		border-left: var(--rule) solid var(--ink);
	}

	.length-bar {
		display: block;
		width: 100%;
		height: 6px;
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
		width: auto;
		min-width: 32px;
		padding: 0 2px;
		border: none;
		background: none;
		color: inherit;
		font-size: 0.75rem;
		font-weight: 500;
		letter-spacing: 0.04em;
		opacity: 0.55;
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

	/*
	 * A row, not a stack: the label is short and the value is long, so side by
	 * side keeps four of these from taking the whole screen. The 44px reach
	 * comes from the base `input` rule — the target *is* the box here, so there
	 * is no overlay to add and nothing beside it to reach over.
	 */
	.entry {
		display: flex;
		align-items: center;
		gap: 12px;
		margin: 0;
	}

	.entry > span {
		flex: none;
		width: 72px;
		padding-left: 12px;
		font-size: 0.65625rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--ink-60);
	}

	/* The `.stack` draws the box, so the input inside an entry draws nothing. */
	.entry input {
		flex: 1;
		min-width: 0;
		padding: 0 12px 0 0;
		border: none;
		background: none;
		font-size: 0.875rem;
	}

	.entry input:focus {
		outline: none;
		background: var(--yellow);
		color: var(--ink);
	}

	.entry input::placeholder {
		color: var(--ink-45);
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
