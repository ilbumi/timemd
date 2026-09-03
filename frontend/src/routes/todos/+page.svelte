<script lang="ts">
	import { goto } from '$app/navigation';
	import Mark from '$lib/Mark.svelte';
	import { api, type Priority, type Todo } from '$lib/api';
	import { attempt } from '$lib/attempt';
	import { today } from '$lib/dates';
	import { group, subtitle } from '$lib/todos';

	const PRIORITIES: Priority[] = ['highest', 'high', 'medium', 'normal', 'low', 'lowest'];

	/** The signifier each priority is written with, so the row shows what the
	    file says rather than a second vocabulary. */
	const PRIORITY_SYMBOLS: Record<Priority, string> = {
		highest: '🔺',
		high: '⏫',
		medium: '🔼',
		normal: '',
		low: '🔽',
		lowest: '⏬'
	};

	let todos = $state<Todo[]>([]);
	let problems = $state<string[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let busy = $state(false);

	/** Off by default: the list is for what is left, not what was done. */
	let showSettled = $state(false);
	/** The one row whose fields are open. A row at a time, because at 360px two
	    open rows is a form nobody can see the top of. */
	let editing = $state<string | null>(null);

	let draftDescription = $state('');
	let draftDue = $state('');

	const groups = $derived(group(todos));
	const openCount = $derived(todos.filter((todo) => todo.status === 'open').length);

	async function load(): Promise<void> {
		error = await attempt(async () => {
			const list = await api.listTodos(showSettled ? {} : { status: 'open' });
			todos = list.todos;
			problems = list.problems;
		});
		loading = false;
	}

	async function run(work: () => Promise<void>): Promise<void> {
		busy = true;
		error = await attempt(work);
		busy = false;
	}

	/**
	 * Ticking stamps the done date, because a `[x]` with no `✅` is a line
	 * nobody would type by hand — and unticking clears it again.
	 */
	const toggle = (todo: Todo): Promise<void> =>
		run(async () => {
			if (todo.id === null) return;
			const done = todo.status === 'done';
			await api.updateTodo(todo.id, {
				status: done ? 'open' : 'done',
				done: done ? null : today()
			});
			await load();
		});

	const add = (): Promise<void> =>
		run(async () => {
			const description = draftDescription.trim();
			if (description === '') return;
			await api.createTodo({ description, due: draftDue === '' ? null : draftDue });
			draftDescription = '';
			draftDue = '';
			await load();
		});

	const remove = (todo: Todo): Promise<void> =>
		run(async () => {
			if (todo.id === null) return;
			await api.deleteTodo(todo.id);
			await load();
		});

	/** Commits one field, and only when it changed — so tabbing through an open
	    row does not write the file once per input. */
	const edit = (todo: Todo, field: 'description' | 'due' | 'scheduled', raw: string) =>
		run(async () => {
			if (todo.id === null) return;
			const value = raw.trim();
			const current = field === 'description' ? todo.description : (todo[field] ?? '');
			if (value === current) return;
			await api.updateTodo(todo.id, {
				[field]: field === 'description' ? value : value === '' ? null : value
			});
			await load();
		});

	const setPriority = (todo: Todo, priority: Priority): Promise<void> =>
		run(async () => {
			if (todo.id === null) return;
			await api.updateTodo(todo.id, { priority });
			await load();
		});

	/** Working on a todo logs a session against its project, with the todo's own
	    words as the note. */
	const start = (todo: Todo): Promise<void> =>
		run(async () => {
			if (todo.id === null) return;
			await api.startSession({ todo: todo.id });
			await goto('/');
		});

	$effect(() => {
		void showSettled;
		void load();
	});
</script>

<section class="screen">
	<header class="head">
		<h1>Todos</h1>
		<p class="meta">{openCount} open</p>
		<button class="quiet" aria-pressed={showSettled} onclick={() => (showSettled = !showSettled)}>
			{showSettled ? 'Hide settled' : 'Show settled'}
		</button>
	</header>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#if problems.length > 0}
		<div class="problems" role="status">
			<strong>{problems.length} line(s) in this file could not be read</strong>
			<ul>
				{#each problems as problem (problem)}
					<li>{problem}</li>
				{/each}
			</ul>
		</div>
	{/if}

	<div class="body">
		{#if loading}
			<p class="empty">Loading…</p>
		{:else if groups.length === 0}
			<p class="empty">Nothing to do.</p>
		{/if}

		{#each groups as band (band.band)}
			<div class="section-head" class:overdue={band.band === 'overdue'}>
				<span class="label">{band.label}</span>
				<span class="meta">{band.todos.length}</span>
			</div>

			<ul class="todos">
				{#each band.todos as todo (todo.id ?? todo.description)}
					{@const settled = todo.status === 'done' || todo.status === 'cancelled'}
					{@const open = editing !== null && editing === todo.id}
					{@const line = subtitle(todo)}
					<li>
						<div class="row">
							<button
								class="tick"
								aria-pressed={settled}
								onclick={() => toggle(todo)}
								disabled={busy || todo.id === null}
							>
								<Mark
									mark="diamond"
									color={settled ? 'var(--red)' : 'var(--ink)'}
									size={18}
									outline={!settled}
								/>
								<span class="text">
									<span class:done={settled}
										>{PRIORITY_SYMBOLS[todo.priority]}{todo.description}</span
									>
									{#if line !== ''}
										<small>{line}</small>
									{/if}
								</span>
							</button>

							<button
								class="quiet"
								aria-label="Start a session on {todo.description}"
								onclick={() => start(todo)}
								disabled={busy || todo.id === null}>▶</button
							>
							<button
								class="quiet"
								aria-label="Edit {todo.description}"
								aria-pressed={open}
								onclick={() => (editing = open ? null : todo.id)}
								disabled={todo.id === null}>⋯</button
							>
						</div>

						{#if open}
							<div class="fields">
								<label class="label" for="description-{todo.id}">Description</label>
								<input
									id="description-{todo.id}"
									type="text"
									value={todo.description}
									onblur={(event) => void edit(todo, 'description', event.currentTarget.value)}
								/>

								<label class="label" for="due-{todo.id}">Due</label>
								<input
									id="due-{todo.id}"
									type="date"
									value={todo.due ?? ''}
									onchange={(event) => void edit(todo, 'due', event.currentTarget.value)}
								/>

								<label class="label" for="scheduled-{todo.id}">Scheduled</label>
								<input
									id="scheduled-{todo.id}"
									type="date"
									value={(todo.scheduled ?? '').slice(0, 10)}
									onchange={(event) => void edit(todo, 'scheduled', event.currentTarget.value)}
								/>

								<label class="label" for="priority-{todo.id}">Priority</label>
								<select
									id="priority-{todo.id}"
									value={todo.priority}
									onchange={(event) =>
										void setPriority(todo, event.currentTarget.value as Priority)}
								>
									{#each PRIORITIES as priority (priority)}
										<option value={priority}>{priority}</option>
									{/each}
								</select>

								<button class="danger wide" onclick={() => remove(todo)} disabled={busy}>
									Delete
								</button>
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{/each}
	</div>

	<div class="adder cluster">
		<input
			type="text"
			placeholder="Add a todo…"
			aria-label="New todo"
			bind:value={draftDescription}
			onkeydown={(event) => {
				if (event.key === 'Enter') {
					event.preventDefault();
					void add();
				}
			}}
		/>
		<input type="date" aria-label="Due date for the new todo" bind:value={draftDue} />
		<button class="primary" onclick={add} disabled={busy || draftDescription.trim() === ''}>
			Add
		</button>
	</div>
</section>

<style>
	.head {
		display: flex;
		align-items: baseline;
		gap: var(--gap);
		padding: 16px var(--pad);
		border-bottom: var(--rule) solid var(--ink);
	}

	.head h1 {
		flex: 1;
		font-size: 1.875rem;
		font-weight: 600;
		line-height: 0.9;
	}

	.body {
		flex: 1;
	}

	.section-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--gap);
		padding: 16px var(--pad) 12px;
		border-bottom: var(--rule) solid var(--ink);
	}

	/* The one band that is not just an ordering: something here is late. */
	.section-head.overdue .label {
		color: var(--red);
	}

	.todos {
		list-style: none;
		margin: 0;
		padding: 0 var(--pad);
	}

	.todos li {
		border-bottom: 1px solid var(--ink-15);
	}

	.row {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	/* The whole row is the target: ticking one-handed should not need a 20px
	   checkbox. Same reason the milestone list works this way. */
	.tick {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
		padding: 12px 0;
		border: none;
		background: none;
		text-align: left;
		text-transform: none;
		letter-spacing: 0;
		font-size: 0.875rem;
	}

	.text {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.text small {
		font-size: 0.6875rem;
		color: var(--ink-45);
	}

	.done {
		color: var(--ink-45);
		text-decoration: line-through;
	}

	.row > .quiet {
		color: var(--ink-45);
	}

	.fields {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 4px 0 14px;
	}

	.fields .label {
		margin-bottom: 0;
	}

	.adder {
		flex-wrap: wrap;
		margin: 12px var(--pad) 16px;
	}

	/*
	 * Native date controls have a large minimum content width. On a phone they
	 * ate the row and left the title as a ~40px square whose placeholder could
	 * not be read. The title takes a full row there; from 500px it shares the
	 * row and the date stays a compact optional field.
	 *
	 * The cluster's shared rule is the one box. Wrapping the title, the generic
	 * sibling `border-left` would land on the date's left — against the cluster's
	 * own left edge — so it is dropped here and put back only on Add, and on
	 * the date once the row is one line.
	 */
	.adder > :is(button, input) + :is(button, input) {
		border-left: none;
	}

	.adder input[type='text'] {
		flex: 1 0 100%;
		width: auto;
		min-width: 0;
		border-bottom: var(--rule) solid var(--ink);
	}

	.adder input[type='date'] {
		flex: 1 1 8rem;
		width: auto;
		min-width: 0;
		padding-inline: 10px;
	}

	.adder .primary {
		flex: none;
		border-left: var(--rule) solid var(--ink);
	}

	@container screen (min-width: 500px) {
		.adder {
			flex-wrap: nowrap;
		}

		.adder input[type='text'] {
			flex: 1 1 0;
			border-bottom: none;
		}

		.adder input[type='date'] {
			flex: 0 0 9.5rem;
			width: 9.5rem;
			border-left: var(--rule) solid var(--ink);
		}
	}

	@container screen (min-width: 900px) {
		.todos,
		.section-head,
		.head,
		.adder {
			max-width: var(--measure);
		}
	}
</style>
