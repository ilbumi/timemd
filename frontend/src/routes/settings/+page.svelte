<script lang="ts">
	import {
		enablePush,
		isIos,
		isStandalone,
		isSubscribed,
		isSupported,
		type PushOutcome
	} from '$lib/notifications';

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

	async function turnOn(): Promise<void> {
		busy = true;
		outcome = await enablePush();
		subscribed = outcome === 'enabled';
		busy = false;
	}

	$effect(() => {
		isSubscribed()
			.then((already) => {
				subscribed = already;
			})
			.catch(() => {
				// Not being able to tell is the same as not subscribed here.
			});
	});
</script>

<h1>Settings</h1>

<section>
	<h2>Reminders</h2>
	<p class="muted">
		Schedule blocks notify you before they start. The lead time is the <code>!5m</code> on each
		block, or the <code>remind_before</code> in <code>settings.md</code>.
	</p>

	{#if mustInstallFirst}
		<div class="notice">
			<strong>Add to Home Screen first</strong>
			<p>
				iOS only delivers notifications to installed apps. Tap Share, then
				<em>Add to Home Screen</em>, and open timemd from there.
			</p>
		</div>
	{:else if !isSupported()}
		<p class="notice">This browser cannot do push notifications.</p>
	{/if}

	<button class="primary" onclick={turnOn} disabled={busy || subscribed || mustInstallFirst}>
		{subscribed ? 'Notifications are on' : 'Turn on notifications'}
	</button>

	{#if outcome}
		<p class="result" class:bad={outcome !== 'enabled'} role="status">{messages[outcome]}</p>
	{/if}
</section>

<section>
	<h2>Your data</h2>
	<p class="muted">
		Everything lives in markdown under the server's data directory — projects in
		<code>projects/</code>, tracked time in <code>days/</code>, repeating blocks in
		<code>schedule/recurring.md</code>. Edit them by hand or with an agent; the app reads them back
		on the next request.
	</p>
</section>

<style>
	section {
		margin-bottom: 24px;
	}

	h2 {
		margin-bottom: 8px;
	}

	p {
		margin: 0 0 var(--gap);
	}

	.muted {
		color: var(--text-muted);
		font-size: 0.9rem;
	}

	.notice {
		padding: 10px 12px;
		margin-bottom: var(--gap);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--surface-raised);
		font-size: 0.9rem;
	}

	.notice p {
		margin: 4px 0 0;
		color: var(--text-muted);
	}

	.result {
		margin-top: 8px;
		font-size: 0.9rem;
	}

	.result.bad {
		color: var(--danger);
	}

	code {
		font-size: 0.85em;
		padding: 1px 4px;
		border-radius: 4px;
		background: var(--surface-sunken);
	}
</style>
