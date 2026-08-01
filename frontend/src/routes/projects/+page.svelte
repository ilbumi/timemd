<script lang="ts">
	import { api, type Project, type ProjectStatus } from '$lib/api';
	import { attempt } from '$lib/attempt';

	let projects = $state<Project[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let newName = $state('');
	let newColor = $state('#4f46e5');
	let editing = $state<string | null>(null);
	let showArchived = $state(false);

	const visible = $derived(
		showArchived ? projects : projects.filter((project) => project.status === 'active')
	);

	async function run(work: () => Promise<void>): Promise<void> {
		error = await attempt(work);
	}

	async function load(): Promise<void> {
		await run(async () => {
			projects = await api.listProjects();
		});
		loading = false;
	}

	async function create(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const name = newName.trim();
		if (name === '') return;

		await run(async () => {
			const created = await api.createProject({ name, color: newColor });
			projects = [...projects, created].sort((left, right) => left.slug.localeCompare(right.slug));
			newName = '';
		});
	}

	async function rename(project: Project, name: string): Promise<void> {
		const trimmed = name.trim();
		if (trimmed === '' || trimmed === project.name) {
			editing = null;
			return;
		}
		await run(async () => {
			replace(await api.updateProject(project.slug, { name: trimmed }));
			editing = null;
		});
	}

	async function recolor(project: Project, color: string): Promise<void> {
		await run(async () => {
			replace(await api.updateProject(project.slug, { color }));
		});
	}

	async function setStatus(project: Project, status: ProjectStatus): Promise<void> {
		await run(async () => {
			replace(await api.updateProject(project.slug, { status }));
		});
	}

	async function remove(project: Project): Promise<void> {
		await run(async () => {
			await api.deleteProject(project.slug);
			projects = projects.filter((candidate) => candidate.slug !== project.slug);
		});
	}

	function replace(updated: Project): void {
		projects = projects.map((project) => (project.slug === updated.slug ? updated : project));
	}

	$effect(() => {
		void load();
	});
</script>

<header>
	<h1>Projects</h1>
	<label class="toggle">
		<input type="checkbox" bind:checked={showArchived} />
		Show archived
	</label>
</header>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}

<form onsubmit={create}>
	<input type="text" placeholder="New project" aria-label="New project name" bind:value={newName} />
	<input type="color" aria-label="New project colour" bind:value={newColor} />
	<button class="primary" type="submit" disabled={newName.trim() === ''}>Add</button>
</form>

{#if loading}
	<p class="muted">Loading…</p>
{:else if visible.length === 0}
	<p class="muted">
		No projects yet. Add one above, or drop a markdown file in <code>projects/</code>.
	</p>
{:else}
	<ul>
		{#each visible as project (project.slug)}
			<li class:archived={project.status === 'archived'}>
				<input
					type="color"
					aria-label="Colour for {project.name}"
					value={project.color ?? '#4f46e5'}
					onchange={(event) => recolor(project, event.currentTarget.value)}
				/>

				{#if editing === project.slug}
					<input
						type="text"
						aria-label="Rename {project.name}"
						value={project.name}
						onblur={(event) => rename(project, event.currentTarget.value)}
						onkeydown={(event) => {
							if (event.key === 'Enter') event.currentTarget.blur();
							if (event.key === 'Escape') editing = null;
						}}
					/>
				{:else}
					<button class="quiet name" onclick={() => (editing = project.slug)}>
						<span>{project.name}</span>
						<small>{project.slug}</small>
					</button>
				{/if}

				<button
					class="quiet"
					title={project.status === 'active' ? 'Archive' : 'Restore'}
					onclick={() => setStatus(project, project.status === 'active' ? 'archived' : 'active')}
				>
					{project.status === 'active' ? 'Archive' : 'Restore'}
				</button>

				{#if project.status === 'archived'}
					<button class="quiet danger" title="Delete" onclick={() => remove(project)}>Delete</button
					>
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--gap);
		margin-bottom: var(--gap);
	}

	.toggle {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-size: 0.8rem;
		white-space: nowrap;
	}

	.toggle input {
		width: auto;
		min-height: auto;
	}

	form {
		display: flex;
		gap: 8px;
		margin-bottom: var(--gap);
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	li {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px;
		background: var(--surface-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
	}

	li.archived {
		opacity: 0.6;
	}

	.name {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0;
		text-align: left;
		min-width: 0;
	}

	.name span {
		font-weight: 600;
	}

	.name small {
		color: var(--text-muted);
		font-size: 0.72rem;
	}

	code {
		font-size: 0.85em;
	}
</style>
