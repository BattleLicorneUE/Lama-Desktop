<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import type { Attachment, DocDraft, Project, ProjectDoc } from "$lib/chat.svelte";

  let { project, code = false, onsave, oncancel }: {
    project: Project | null;
    /** Projet de code : lié à un dossier de travail. */
    code?: boolean;
    onsave: (name: string, instructions: string, docs: DocDraft[], removed: string[], workspace: string) => void;
    oncancel: () => void;
  } = $props();

  // Le dialogue est recréé à chaque ouverture : seule la valeur initiale compte.
  // svelte-ignore state_referenced_locally
  let name = $state(project?.name ?? "");
  // svelte-ignore state_referenced_locally
  let instructions = $state(project?.instructions ?? "");
  // svelte-ignore state_referenced_locally
  let workspace = $state(project?.workspace ?? "");
  let docs = $state<DocDraft[]>([]);
  let removed = $state<string[]>([]);
  let error = $state("");
  const valid = $derived(name.trim().length > 0 && (!code || workspace.trim().length > 0));

  async function pickFolder() {
    const sel = await openDialog({ directory: true, title: "Dossier du projet" });
    if (typeof sel === "string") {
      workspace = sel;
      if (!name.trim()) name = sel.split(/[\\/]/).filter(Boolean).pop() ?? "";
    }
  }

  onMount(async () => {
    if (project) {
      const existing = await invoke<ProjectDoc[]>("project_docs", { projectId: project.id });
      docs = existing.map((d) => ({ id: d.id, name: d.name, content: d.content }));
    }
  });

  async function addDocs() {
    error = "";
    const sel = await openDialog({ multiple: true, title: "Ajouter des documents au projet" });
    if (!sel) return;
    for (const path of Array.isArray(sel) ? sel : [sel]) {
      try {
        const a = await invoke<Attachment>("read_attachment", { path });
        if (a.kind === "text") docs.push({ name: a.name, content: a.data });
        else error = `${a.name} : seuls les fichiers texte sont acceptés.`;
      } catch (e) {
        error = String(e);
      }
    }
  }

  function removeDoc(i: number) {
    const [d] = docs.splice(i, 1);
    if (d.id) removed.push(d.id);
  }

  function submit(e: Event) {
    e.preventDefault();
    if (valid) onsave(name.trim(), instructions, $state.snapshot(docs), $state.snapshot(removed), workspace.trim());
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && oncancel()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" onmousedown={(e) => e.target === e.currentTarget && oncancel()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={project ? "Modifier le projet" : "Nouveau projet"}>
  <form onsubmit={submit}>
    <h2>{project ? "Modifier le projet" : "Nouveau projet"}</h2>
    <label>
      Nom
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={name} placeholder="Ex. Application mobile" autofocus />
    </label>
    {#if code}
      <label>
        Dossier de travail
        <div class="folder">
          <input bind:value={workspace} placeholder="C:\Users\moi\mon-projet" spellcheck="false" />
          <button type="button" class="ghost" onclick={pickFolder}>Parcourir…</button>
        </div>
      </label>
    {/if}
    <label>
      Instructions
      <textarea bind:value={instructions} rows="5" placeholder={code ? "Consignes pour toutes les sessions de ce projet (conventions, commandes, règles…)" : "Contexte et consignes appliqués à tous les chats de ce projet (ton, langue, règles…)"}></textarea>
    </label>
    <div class="docs">
      <div class="dh">
        <span>Documents de référence</span>
        <button type="button" class="ghost" onclick={addDocs}>+ Ajouter</button>
      </div>
      {#each docs as d, i}
        <div class="doc">📄 <span class="dn">{d.name}</span><span class="size">{Math.max(1, Math.round(d.content.length / 1000))} Ko</span>
          <button type="button" aria-label="Retirer {d.name}" onclick={() => removeDoc(i)}>×</button></div>
      {:else}
        <p class="muted">Ils sont ajoutés au contexte de chaque {code ? "session" : "chat"} du projet (40 Ko max au total).</p>
      {/each}
      {#if error}<p class="err">{error}</p>{/if}
    </div>
    <div class="actions">
      <button type="button" class="ghost" onclick={oncancel}>Annuler</button>
      <button type="submit" class="primary" disabled={!valid}>{project ? "Enregistrer" : "Créer le projet"}</button>
    </div>
  </form>
  </div>
</div>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .dialog { max-width: 500px; padding: 22px; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 22px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--muted); }
  input, textarea { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 9px 12px; outline: none; color: var(--fg); resize: vertical; font-size: 15px; }
  input:focus, textarea:focus { border-color: var(--accent); }
  .folder { display: flex; gap: 8px; }
  .folder input { flex: 1; min-width: 0; }
  .folder button { border: 1px solid var(--border); border-radius: 10px; padding: 0 14px; background: none; color: var(--fg); }
  .docs { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--muted); }
  .dh { display: flex; justify-content: space-between; align-items: center; }
  .doc { display: flex; align-items: center; gap: 8px; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; padding: 5px 6px 5px 10px; color: var(--fg); }
  .dn { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .size { color: var(--muted); font-size: 12px; }
  .doc button { border: 0; background: none; color: var(--muted); font-size: 16px; padding: 0 6px; }
  .muted { margin: 0; }
  .err { margin: 0; color: var(--del); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .actions button, .dh button { border-radius: 10px; padding: 8px 16px; border: 1px solid var(--border); }
  .dh button { padding: 3px 10px; font-size: 13px; }
  .ghost { background: none; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .primary:disabled { opacity: 0.4; }
</style>
