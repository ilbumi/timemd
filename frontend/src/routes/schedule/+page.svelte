<script lang="ts">
	import { ApiError, api, type Project, type RecurringBlock } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { clockTime } from '$lib/dates';

	let blocks = $state<RecurringBlock[]>([]);
	let projects = $state<Project[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let dirty = $state(false);

	const blank = (): RecurringBlock => ({
		id: '',
		days: 'mon-fri',
		start: '09:00:00',
		end: '10:00:00',
		project: null,
		title: '',
		remindBefore: '5m'
	});

	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(work);
	}

	async function load(): Promise<void> {
		await run(async () => {
			blocks = await api.readRecurring();
		});
		loading = false;
	}

	const add = (): void => {
		blocks = [...blocks, blank()];
		dirty = true;
	};

	const remove = (index: number): void => {
		blocks = blocks.filter((_, position) => position !== index);
		dirty = true;
	};

	const save = (): Promise<void> =>
		run(async () => {
			// Blocks with no id would be dropped by the server anyway; refusing here
			// gives a clearer message than a validation error from the grammar.
			const unnamed = blocks.findIndex((block) => block.id.trim() === '');
			if (unnamed !== -1) {
				throw new ApiError(400, `Block ${unnamed + 1} needs an id`);
			}
			blocks = await api.writeRecurring(blocks);
			dirty = false;
		});

	$effect(() => {
		void load();
		api
			.listActiveProjects()
			.then((active) => {
				projects = active;
			})
			.catch(() => {
				// Editing works without the project list; the field just stays empty.
			});
	});
</script>

<header>
	<h1>Schedule</h1>
	<button class="primary" onclick={save} disabled={!dirty}>Save</button>
</header>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}

{#if loading}
	<p class="muted">Loading…</p>
{:else}
	{#if blocks.length === 0}
		<p class="muted">
			No repeating blocks yet. These live in <code>schedule/recurring.md</code>, one line each.
		</p>
	{/if}

	<ul>
		{#each blocks as block, index (index)}
			<li>
				<div class="row">
					<input
						type="text"
						placeholder="id"
						aria-label="Block id for row {index + 1}"
						bind:value={block.id}
						oninput={() => (dirty = true)}
					/>
					<input
						type="text"
						placeholder="mon-fri"
						aria-label="Days for row {index + 1}"
						bind:value={block.days}
						oninput={() => (dirty = true)}
					/>
				</div>

				<div class="row">
					<input
						type="time"
						aria-label="Start for row {index + 1}"
						value={clockTime(block.start)}
						onchange={(event) => {
							block.start = `${event.currentTarget.value}:00`;
							dirty = true;
						}}
					/>
					<input
						type="time"
						aria-label="End for row {index + 1}"
						value={clockTime(block.end)}
						onchange={(event) => {
							block.end = `${event.currentTarget.value}:00`;
							dirty = true;
						}}
					/>
				</div>

				<input
					type="text"
					placeholder="Title"
					aria-label="Title for row {index + 1}"
					bind:value={block.title}
					oninput={() => (dirty = true)}
				/>

				<div class="row">
					<select
						aria-label="Project for row {index + 1}"
						value={block.project ?? ''}
						onchange={(event) => {
							block.project = event.currentTarget.value || null;
							dirty = true;
						}}
					>
						<option value="">No project</option>
						{#each projects as project (project.slug)}
							<option value={project.slug}>{project.name}</option>
						{/each}
					</select>
					<input
						type="text"
						placeholder="!5m"
						aria-label="Reminder lead for row {index + 1}"
						value={block.remindBefore ?? ''}
						onchange={(event) => {
							block.remindBefore = event.currentTarget.value || null;
							dirty = true;
						}}
					/>
					<button class="quiet danger" onclick={() => remove(index)}>Delete</button>
				</div>
			</li>
		{/each}
	</ul>

	<button onclick={add}>Add a repeating block</button>
{/if}

<style>
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--gap);
		margin-bottom: var(--gap);
	}

	ul {
		list-style: none;
		margin: 0 0 var(--gap);
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--gap);
	}

	li {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px;
		background: var(--surface-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
	}

	.row {
		display: flex;
		gap: 8px;
	}

	.row > * {
		flex: 1;
		min-width: 0;
	}

	code {
		font-size: 0.85em;
	}
</style>
