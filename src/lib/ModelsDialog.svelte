<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { CAP_LABELS, type ChatStore } from "$lib/chat.svelte";
  import ModelGuide from "$lib/ModelGuide.svelte";

  let { chat, onclose }: { chat: ChatStore; onclose: () => void } = $props();

  type Details = {
    details?: { family?: string; parameter_size?: string; quantization_level?: string };
    capabilities?: string[];
  };

  let filter = $state("");
  let need = $state("");
  let tab = $state<"list" | "guide">("list");

  onMount(() => {
    chat.loadAllCaps();
  });
  let info = $state<{ name: string; data: Details } | null>(null);
  let error = $state("");

  const shown = $derived(
    chat.models.filter((m) => m.name.toLowerCase().includes(filter.trim().toLowerCase()) && (!need || chat.caps[m.name]?.includes(need))),
  );

  async function show(name: string) {
    error = "";
    try {
      info = { name, data: await invoke<Details>("model_show", { name, cloudKey: chat.webKey }) };
    } catch (e) {
      error = String(e);
      if (error.includes("410")) chat.unavailable[name] = true;
    }
  }

  function use(name: string) {
    chat.setModel(name);
    onclose();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Modèles">
    <div class="head">
      <h2>Modèles Ollama Cloud</h2>
      <button class="x" onclick={onclose} aria-label="Fermer">×</button>
    </div>

    <div class="chips" role="tablist" aria-label="Affichage">
      <button class="chip" role="tab" aria-selected={tab === "list"} class:on={tab === "list"} onclick={() => (tab = "list")}>Mes modèles</button>
      <button class="chip" role="tab" aria-selected={tab === "guide"} class:on={tab === "guide"} onclick={() => (tab = "guide")}>Guide par usage et prix</button>
    </div>

    {#if tab === "guide"}
      <div class="guidebox"><ModelGuide {chat} onpick={onclose} /></div>
    {:else if !chat.webKey}
      <p class="warn">Pas de clé API : ajoute-la dans Réglages → Ollama Cloud pour voir les modèles.</p>
    {:else}
      <input bind:value={filter} placeholder="Filtrer les modèles…" aria-label="Filtrer les modèles" />
      <div class="chips" role="group" aria-label="Filtrer par capacité">
        <button class="chip" class:on={!need} onclick={() => (need = "")}>Tous</button>
        {#each ["vision", "tools", "thinking"] as c}
          <button class="chip" class:on={need === c} onclick={() => (need = need === c ? "" : c)}>{CAP_LABELS[c]}</button>
        {/each}
      </div>
      {#if error}<p class="warn">{error}</p>{/if}
      <ul class="list">
        {#each shown as m (m.name)}
          <li class:current={m.name === chat.model}>
            <button class="row" onclick={() => show(m.name)}>
              <span class="n">{m.name}</span>
              {#each chat.capsOf(m.name) as c}<span class="badge cap">{CAP_LABELS[c]}</span>{/each}
              {#if m.name === chat.model}<span class="badge now">En cours</span>{/if}
              {#if chat.unavailable[m.name]}<span class="badge off" title="Ollama répond 410 : ce modèle n'est plus proposé">Indisponible</span>{/if}
            </button>
            <button class="use" onclick={() => use(m.name)} disabled={chat.busy || !!chat.unavailable[m.name]}>Utiliser</button>
          </li>
        {:else}
          <li class="muted empty">{chat.models.length ? "Aucun modèle ne correspond." : "Aucun modèle chargé."}</li>
        {/each}
      </ul>
    {/if}

    {#if info}
      <div class="info">
        <strong>{info.name}</strong>
        <div class="muted">
          {[info.data.details?.family, info.data.details?.parameter_size, info.data.details?.quantization_level]
            .filter(Boolean).join(" · ")}
        </div>
        {#if info.data.capabilities?.length}
          <div class="caps">{#each info.data.capabilities as c}<span class="badge">{c}</span>{/each}</div>
          {#if !info.data.capabilities.includes("tools")}
            <p class="muted small">Ce modèle ne gère pas les outils : le mode Code fonctionnera en simple chat.</p>
          {/if}
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .dialog { display: flex; flex-direction: column; gap: 12px; }
  .guidebox { overflow-y: auto; max-height: 60vh; padding-right: 4px; }
  .head { display: flex; justify-content: space-between; align-items: center; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 22px; }
  .x { border: 0; background: none; font-size: 22px; color: var(--muted); }
  input { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 8px 12px; outline: none; }
  input:focus { border-color: var(--accent); }
  .muted { color: var(--muted); font-size: 13px; }
  .small { margin: 4px 0 0; }
  .warn { color: var(--del); margin: 0; font-size: 14px; }
  .list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 12px; overflow: hidden; max-height: 340px; overflow-y: auto; }
  li { display: flex; align-items: center; border-bottom: 1px solid var(--border); }
  li:last-child { border-bottom: 0; }
  li.empty { padding: 12px; }
  li.current { background: var(--hover); }
  .row { flex: 1; display: flex; gap: 10px; align-items: center; border: 0; background: none; padding: 10px 12px; text-align: left; }
  .row:hover { background: var(--hover); }
  .n { flex: 1; }
  .use { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 4px 12px; margin-right: 10px; }
  .use:hover:not(:disabled) { border-color: var(--accent); }
  .use:disabled { opacity: 0.45; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 3px 12px; font-size: 13px; }
  .chip.on { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .badge.now { color: var(--accent); }
  .badge.off { color: var(--del); }
  .badge { font-size: 12px; background: var(--hover); color: var(--muted); border-radius: 6px; padding: 1px 7px; }
  .info { background: var(--surface); border: 1px solid var(--border); border-radius: 12px; padding: 12px; display: flex; flex-direction: column; gap: 6px; }
  .caps { display: flex; gap: 6px; flex-wrap: wrap; }
</style>
