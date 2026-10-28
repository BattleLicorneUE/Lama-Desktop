<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ChatStore } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";
  import AgentEditor from "$lib/AgentEditor.svelte";

  let { chat }: { chat: ChatStore } = $props();

  let editor = $state<{ from: string | null; copy: boolean } | null>(null);
  let msg = $state("");
  let query = $state("");

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? chat.agents.filter((a) => (a.name + " " + a.description).toLowerCase().includes(q)) : chat.agents;
  });

  const scopeLabel = (s: string) => (s === "builtin" ? "intégré" : s === "project" ? "projet" : "perso");

  async function removeAgent(name: string) {
    if (!(await askConfirm(`Supprimer l'agent « ${name} » ? Son fichier est effacé de ce PC.`, "Supprimer"))) return;
    try {
      await invoke("agent_delete", { workspace: chat.mode === "code" && chat.workspace ? chat.workspace : null, name });
      await chat.loadAgents();
      msg = `Agent « ${name} » supprimé.`;
    } catch (err) {
      msg = String(err);
    }
  }

  async function importClaude() {
    try {
      const n = await invoke<number>("agents_import_claude");
      msg = n ? `${n} agent${n > 1 ? "s" : ""} importé${n > 1 ? "s" : ""} depuis Claude Code.` : "Aucun nouvel agent trouvé dans ~/.claude/agents.";
      await chat.loadAgents();
    } catch (err) {
      msg = String(err);
    }
  }
</script>

<div class="panel">
  <p class="muted">Un agent, c'est un rôle : des consignes, une liste d'outils autorisés et, si tu veux, son propre modèle. Tu n'as rien à activer.
    Selon la tâche, l'agent principal décide <strong>seul</strong> de confier une partie du travail à l'un d'eux (ils travaillent en parallèle, chacun avec sa propre mémoire, et rendent un résumé) ou de la faire lui-même.
    La <em>description</em> de chaque agent lui dit quand s'en servir. Même format que les agents de Claude Code (fichiers <code>.md</code>), ceux d'un projet vont dans <code>.agents/</code>.</p>
  {#if msg}<p class="msg">{msg}</p>{/if}

  <section>
    <div class="head">
      <h3>Agents ({chat.agents.length})</h3>
      <div class="row">
        <button class="primary" onclick={() => (editor = { from: null, copy: false })}>+ Nouvel agent</button>
        <button onclick={importClaude}>Importer ceux de Claude</button>
        <button onclick={() => invoke("agents_open_dir")}>Ouvrir le dossier</button>
        <button onclick={() => chat.loadAgents()}>Actualiser</button>
      </div>
    </div>

    {#if chat.agents.length > 6}
      <input class="find" bind:value={query} placeholder="Rechercher un agent…" aria-label="Rechercher un agent" />
    {/if}
    <ul class="scroll">
      {#each shown as a (a.name)}
        <li>
          <div class="grow">
            <strong>{a.name}</strong>
            <span class="badge">{scopeLabel(a.scope)}</span>
            {#if a.model}<span class="badge">{a.model}</span>{/if}
            {#if a.tools}<span class="badge" title={a.tools.join(", ")}>{a.tools.length} outils</span>{/if}
            {#if a.permission === "plan"}<span class="badge">lecture seule</span>{/if}
            <div class="muted clip" title={a.description}>{a.description || "Sans description"}</div>
          </div>
          {#if a.scope === "builtin"}
            <button class="mini" onclick={() => (editor = { from: a.name, copy: true })} aria-label="Faire ta version de {a.name}" title="Les agents intégrés ne se modifient pas : tu en fais ta propre version.">Copier</button>
          {:else}
            <button class="mini" onclick={() => (editor = { from: a.name, copy: false })} aria-label="Modifier {a.name}">Modifier</button>
            <button class="mini" onclick={() => removeAgent(a.name)} aria-label="Supprimer {a.name}">Supprimer</button>
          {/if}
        </li>
      {:else}
        <li class="muted">Aucun agent ne correspond.</li>
      {/each}
    </ul>
  </section>
</div>

{#if editor}
  <AgentEditor {chat} from={editor.from} copy={editor.copy} onclose={() => (editor = null)} onsaved={(m) => { msg = m; editor = null; }} />
{/if}

<style>
  .panel { display: flex; flex-direction: column; gap: 14px; }
  section { display: flex; flex-direction: column; gap: 10px; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 10px; flex-wrap: wrap; }
  h3 { margin: 0; font-size: 14px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .msg { margin: 0; font-size: 13px; color: var(--accent); }
  code { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12.5px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .grow { flex: 1; min-width: 0; }
  .clip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  input.find { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 7px 10px; outline: none; }
  input.find:focus { border-color: var(--accent); }
  .mini { padding: 3px 9px; font-size: 12.5px; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; }
  button:hover:not(:disabled) { border-color: var(--accent); }
  button:disabled { opacity: 0.45; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 10px; }
  ul.scroll { max-height: 340px; overflow-y: auto; }
  li { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid var(--border); font-size: 14px; }
  li:last-child { border-bottom: 0; }
  .badge { font-size: 12px; background: var(--hover); color: var(--muted); border-radius: 5px; padding: 0 6px; margin-left: 6px; }
</style>
