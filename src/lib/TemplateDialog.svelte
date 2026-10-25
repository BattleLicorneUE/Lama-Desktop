<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import type { ChatStore } from "$lib/chat.svelte";

  let { chat, onclose }: { chat: ChatStore; onclose: () => void } = $props();

  type Tpl = { id: string; label: string; desc: string; prompt: string };
  let list = $state<Tpl[]>([]);
  let error = $state("");
  let busy = $state(false);

  onMount(async () => {
    try {
      list = await invoke<Tpl[]>("project_templates");
    } catch (e) {
      error = String(e);
    }
  });

  async function use(t: Tpl) {
    const dir = await openDialog({ directory: true, title: `Dossier du projet « ${t.label} » (vide de préférence)` });
    if (typeof dir !== "string") return;
    busy = true;
    error = "";
    try {
      await invoke("project_from_template", { id: t.id, dir });
      chat.workspace = dir;
      chat.draft = t.prompt;
      chat.flash("Projet créé : décris ce que tu veux construire.");
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()} onkeydown={(e) => e.key === "Escape" && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Modèles de projet" tabindex="-1">
    <div class="head"><h2>Partir d'un modèle</h2><button class="x" onclick={onclose} aria-label="Fermer">×</button></div>
    <p class="muted">Quelques fichiers de départ, avec un fichier AGENTS.md de règles. Tu choisis un dossier ; rien n'est écrasé.</p>
    <ul>
      {#each list as t (t.id)}
        <li>
          <span class="grow"><strong>{t.label}</strong><br /><span class="muted">{t.desc}</span></span>
          <button onclick={() => use(t)} disabled={busy}>Choisir un dossier…</button>
        </li>
      {/each}
    </ul>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
</div>

<style>
  .dialog { max-width: 520px; display: flex; flex-direction: column; gap: 12px; }
  .head { display: flex; justify-content: space-between; align-items: center; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 22px; }
  .x { border: 0; background: none; font-size: 22px; color: var(--muted); }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  li { display: flex; align-items: center; gap: 12px; border: 1px solid var(--border); border-radius: 12px; padding: 10px 12px; background: var(--surface); }
  .grow { flex: 1; min-width: 0; font-size: 14px; }
  li button { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 6px 12px; flex: none; }
  li button:hover { border-color: var(--accent); }
  .err { color: var(--del); font-size: 13px; margin: 0; }
</style>
