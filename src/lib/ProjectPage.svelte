<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ChatStore, ProjectDoc } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";
  import { projectBlurb, shortDate } from "$lib/projects";
  import { splitProjectDocs } from "$lib/attachments";
  import Composer from "$lib/Composer.svelte";
  import Icon from "$lib/Icon.svelte";

  let { chat }: { chat: ChatStore } = $props();
  const project = $derived(chat.currentProject ?? chat.projects.find((p) => p.id === chat.activeProject));
  let docs = $state<ProjectDoc[]>([]);

  // Les documents se rechargent à l'ouverture du projet et après chaque modification (la date de mise à jour change).
  $effect(() => {
    const id = project?.id;
    void project?.updated_at;
    if (!id) {
      docs = [];
      return;
    }
    invoke<ProjectDoc[]>("project_docs", { projectId: id }).then((d) => (docs = d)).catch(() => (docs = []));
  });

  let newMemory = $state("");
  $effect(() => {
    if (project?.id) chat.loadProjectMemories(project.id);
  });

  async function addMemory() {
    if (!project || !newMemory.trim()) return;
    await chat.addProjectMemory(project.id, newMemory);
    newMemory = "";
  }

  const indexedCount = $derived(splitProjectDocs(docs).indexed.length);

  async function remove() {
    if (project && (await askConfirm(`Supprimer le projet « ${project.name} » ? Ses chats sont conservés.`))) await chat.deleteProject(project.id);
  }
</script>

{#if project}
  <div class="page">
    <button class="back" onclick={() => chat.showProjects()}><Icon name="back" size={15} /> Projets</button>
    <header>
      <div class="titles">
        <h1>{project.name}</h1>
        <p>{projectBlurb(project.instructions)}</p>
      </div>
      <div class="acts">
        <button class="icon" class:on={project.pinned} onclick={() => chat.pinProject(project)} title={project.pinned ? "Désépingler" : "Épingler en haut de la liste"} aria-label={project.pinned ? "Désépingler le projet" : "Épingler le projet"} aria-pressed={project.pinned}><Icon name="pin" size={17} /></button>
        <button class="icon" onclick={() => (chat.projectDialog = project)} title="Modifier le projet" aria-label="Modifier le projet"><Icon name="pencil" size={17} /></button>
        <button class="icon" onclick={remove} title="Supprimer le projet" aria-label="Supprimer le projet"><Icon name="trash" size={17} /></button>
      </div>
    </header>

    <div class="cols">
      <section class="left">
        <div class="box"><Composer {chat} placeholder="{chat.mode === "code" ? "Nouvelle session" : "Nouvelle conversation"} dans {project.name}" /></div>
        <h2>Récents</h2>
        <ul class="recent">
          {#each chat.convs as c (c.id)}
            <li><button onclick={() => chat.open(c)}><span class="t">{c.title}</span><span class="d">{shortDate(c.updated_at)}</span></button></li>
          {:else}
            <li class="none">Aucune conversation dans ce projet pour l'instant.</li>
          {/each}
        </ul>
      </section>

      <aside>
        {#if project.workspace}
          <div class="row">
            <Icon name="folder" size={17} /><span class="k">Dossier</span>
            <button class="act" onclick={() => (chat.projectDialog = project)}>Modifier</button>
          </div>
          <p class="excerpt path" title={project.workspace}>{project.workspace}</p>
        {/if}
        <div class="row">
          <Icon name="pencil" size={17} /><span class="k">Instructions</span>
          <button class="act" onclick={() => (chat.projectDialog = project)}>{project.instructions.trim() ? "Modifier" : "Ajouter"}</button>
        </div>
        {#if project.instructions.trim()}<p class="excerpt">{project.instructions.trim()}</p>{/if}
        <div class="row">
          <Icon name="file" size={17} /><span class="k">Contexte</span>
          <button class="act" onclick={() => (chat.projectDialog = project)}>{docs.length ? "Modifier" : "Ajouter"}</button>
        </div>
        {#if docs.length}
          <ul class="docs">{#each docs as d (d.id)}<li title={d.name}>{d.name}</li>{/each}</ul>
          {#if indexedCount}<p class="excerpt">{indexedCount} document{indexedCount > 1 ? "s" : ""} volumineux : indexé{indexedCount > 1 ? "s" : ""}, le modèle n'en lit que les passages utiles.</p>{/if}
        {/if}
        <div class="row">
          <Icon name="book" size={17} /><span class="k">Mémoire</span>
        </div>
        <p class="excerpt memhint">Ce que Lama retient de ce projet : relu dans chacune de ses conversations. Le modèle peut l'enrichir seul.</p>
        <ul class="mem">
          {#each chat.projectMemories as m (m.id)}
            <li><span>{m.text}</span><button class="x" aria-label="Oublier" onclick={() => chat.deleteProjectMemory(project.id, m.id)}><Icon name="close" size={13} /></button></li>
          {/each}
        </ul>
        <form class="addmem" onsubmit={(e) => { e.preventDefault(); addMemory(); }}>
          <input bind:value={newMemory} placeholder="Ajouter un souvenir…" aria-label="Ajouter à la mémoire du projet" />
          <button disabled={!newMemory.trim()}>Ajouter</button>
        </form>
        <p class="private">Privé</p>
      </aside>
    </div>
  </div>
{/if}

<style>
  .page { flex: 1; overflow: auto; padding: 36px max(24px, calc((100% - 1040px) / 2)) 40px; }
  .back { display: inline-flex; align-items: center; gap: 4px; border: 0; background: none; color: var(--muted); font-size: 14px; padding: 4px 6px 4px 0; margin-bottom: 14px; }
  .back:hover { color: var(--fg); }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; margin-bottom: 26px; }
  h1 { margin: 0 0 6px; font-size: 28px; font-weight: 500; font-family: Georgia, "Times New Roman", serif; }
  .titles p { margin: 0; color: var(--muted); font-size: 15px; }
  .acts { display: flex; gap: 2px; }
  .icon { display: grid; place-items: center; border: 0; background: none; color: var(--muted); width: 34px; height: 34px; border-radius: 8px; }
  .icon:hover { background: var(--hover); color: var(--fg); }
  .icon.on { color: var(--accent); }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) 300px; gap: 48px; }
  @media (max-width: 860px) { .cols { grid-template-columns: 1fr; gap: 28px; } }
  .box { margin-bottom: 26px; }
  h2 { margin: 0 0 8px; font-size: 14px; font-weight: 400; color: var(--muted); }
  .recent { list-style: none; margin: 0; padding: 0; }
  .recent button { display: flex; width: 100%; justify-content: space-between; gap: 16px; border: 0; background: none; color: var(--fg); padding: 9px 8px; border-radius: 8px; text-align: left; font-size: 14.5px; }
  .recent button:hover { background: var(--hover); }
  .t { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .d { color: var(--muted); flex: none; }
  .none { color: var(--muted); font-size: 14px; padding: 8px; }
  aside { padding-top: 6px; }
  .row { display: flex; align-items: center; gap: 12px; padding: 14px 0; border-bottom: 1px solid var(--border); }
  .k { flex: 1; font-size: 15px; }
  .act { border: 0; background: none; color: var(--fg); font-size: 14px; padding: 2px 4px; border-radius: 6px; }
  .act:hover { background: var(--hover); }
  .excerpt { margin: 8px 0 0; color: var(--muted); font-size: 13.5px; line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; white-space: pre-wrap; }
  .excerpt.path { white-space: normal; word-break: break-all; }
  .docs { list-style: none; margin: 8px 0 0; padding: 0; color: var(--muted); font-size: 13.5px; display: grid; gap: 4px; }
  .docs li { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .memhint { -webkit-line-clamp: unset; line-clamp: unset; }
  .mem { list-style: none; margin: 8px 0 0; padding: 0; display: grid; gap: 4px; }
  .mem li { display: flex; justify-content: space-between; gap: 8px; color: var(--muted); font-size: 13.5px; line-height: 1.4; }
  .x { flex: none; border: 0; background: none; color: var(--muted); border-radius: 6px; padding: 2px 4px; height: fit-content; }
  .x:hover { background: var(--hover); color: var(--fg); }
  .addmem { display: flex; gap: 6px; margin-top: 10px; }
  .addmem input { flex: 1; min-width: 0; border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 6px 9px; outline: none; color: var(--fg); font-size: 13.5px; }
  .addmem input:focus { border-color: var(--accent); }
  .addmem button { border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 8px; padding: 5px 10px; font-size: 13.5px; }
  .addmem button:disabled { opacity: 0.45; }
  .private { margin: 18px 0 0; color: var(--muted); font-size: 13px; }
</style>
