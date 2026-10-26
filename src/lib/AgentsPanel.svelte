<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ChatStore } from "$lib/chat.svelte";

  let { chat }: { chat: ChatStore } = $props();

  let creating = $state(false);
  let name = $state("");
  let description = $state("");
  let prompt = $state("");
  let readOnly = $state(false);
  let msg = $state("");
  let query = $state("");

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? chat.agents.filter((a) => (a.name + " " + a.description).toLowerCase().includes(q)) : chat.agents;
  });

  const scopeLabel = (s: string) => (s === "builtin" ? "intégré" : s === "project" ? "projet" : "perso");

  async function create(e: Event) {
    e.preventDefault();
    try {
      await invoke("agent_create", { name, description, prompt, readOnly });
      msg = `Agent « ${name.trim()} » créé.`;
      creating = false;
      name = description = prompt = "";
      readOnly = false;
      await chat.loadAgents();
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
        <button onclick={() => (creating = !creating)}>{creating ? "Annuler" : "Nouvel agent"}</button>
        <button onclick={importClaude}>Importer ceux de Claude</button>
        <button onclick={() => invoke("agents_open_dir")}>Ouvrir le dossier</button>
        <button onclick={() => chat.loadAgents()}>Actualiser</button>
      </div>
    </div>

    {#if creating}
      <form class="create" onsubmit={create}>
        <input bind:value={name} placeholder="Nom (ex. relecteur-securite)" aria-label="Nom de l'agent" required />
        <input bind:value={description} placeholder="Description : quand l'utiliser" aria-label="Description" />
        <textarea bind:value={prompt} rows="5" placeholder="Consigne : son rôle et sa méthode" aria-label="Consigne"></textarea>
        <label class="check"><input type="checkbox" bind:checked={readOnly} /> Lecture seule (lire, chercher, lister — rien modifier)</label>
        <div class="row end"><button class="primary" disabled={!name.trim()}>Créer</button></div>
      </form>
    {/if}

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
        </li>
      {:else}
        <li class="muted">Aucun agent ne correspond.</li>
      {/each}
    </ul>
  </section>
</div>

<style>
  .panel { display: flex; flex-direction: column; gap: 14px; }
  section { display: flex; flex-direction: column; gap: 10px; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 10px; flex-wrap: wrap; }
  h3 { margin: 0; font-size: 14px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .msg { margin: 0; font-size: 13px; color: var(--accent); }
  code { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12.5px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .row.end { justify-content: flex-end; }
  .grow { flex: 1; min-width: 0; }
  .clip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .create { display: flex; flex-direction: column; gap: 8px; border: 1px solid var(--border); border-radius: 12px; padding: 12px; background: var(--surface); }
  input:not([type="checkbox"]), textarea { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 7px 10px; outline: none; resize: vertical; }
  input:focus, textarea:focus { border-color: var(--accent); }
  .check { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--muted); }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; }
  button:hover:not(:disabled) { border-color: var(--accent); }
  button:disabled { opacity: 0.45; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 10px; }
  ul.scroll { max-height: 340px; overflow-y: auto; }
  li { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid var(--border); font-size: 14px; }
  li:last-child { border-bottom: 0; }
  .badge { font-size: 11px; background: var(--hover); color: var(--muted); border-radius: 5px; padding: 0 6px; margin-left: 6px; }
</style>
