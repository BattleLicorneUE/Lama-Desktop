<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ChatStore } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";
  import { parseJsonObject } from "$lib/lama-draft";

  /** Une ligne « décris ton idée » qui fait rédiger le formulaire par le modèle. `filled` : le formulaire contient déjà du texte à ne pas écraser sans demander. */
  let { chat, system, placeholder, subject, filled = false, onfill }: {
    chat: ChatStore;
    system: string;
    placeholder: string;
    /** Ce que Lama rédige, pour les messages (« la tâche », « les instructions »). */
    subject: string;
    filled?: boolean;
    onfill: (fields: Record<string, string>) => void;
  } = $props();

  let idea = $state("");
  let busy = $state(false);
  let error = $state("");

  async function write() {
    error = "";
    if (!idea.trim()) return;
    if (!chat.model) {
      error = `Choisis d'abord un modèle dans la conversation : c'est lui qui rédige ${subject}.`;
      return;
    }
    if (filled && !(await askConfirm(`Lama va remplacer ce que tu as déjà écrit pour ${subject}. Continuer ?`, "Remplacer"))) return;
    busy = true;
    try {
      const out = await invoke<string>("chat_once", {
        model: chat.model,
        cloudKey: chat.webKey,
        options: { temperature: 0.4 },
        messages: [
          { role: "system", content: system },
          { role: "user", content: idea.trim() },
        ],
      });
      const fields = parseJsonObject(out);
      if (!fields || !Object.values(fields).some((v) => v.trim())) {
        error = `Lama n'a pas réussi à rédiger ${subject}. Reformule ton idée ou réessaie.`;
        return;
      }
      onfill(fields);
    } catch (e) {
      error = `La rédaction a échoué : ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      busy = false;
    }
  }
</script>

<div class="draft">
  <form onsubmit={(e) => { e.preventDefault(); void write(); }}>
    <input bind:value={idea} {placeholder} disabled={busy} aria-label="Ton idée, en une phrase" />
    <button disabled={!idea.trim() || busy}>{busy ? "Rédaction…" : "Rédiger avec Lama"}</button>
  </form>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</div>

<style>
  .draft { display: flex; flex-direction: column; gap: 6px; padding: 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--code); }
  form { display: flex; gap: 8px; }
  input { flex: 1; min-width: 0; border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 8px; padding: 7px 10px; outline: none; font: inherit; font-size: 14px; }
  input:focus { border-color: var(--accent); }
  button { border: 1px solid var(--accent); background: var(--accent); color: var(--accent-fg); border-radius: 8px; padding: 6px 12px; font: inherit; font-size: 13.5px; cursor: pointer; white-space: nowrap; }
  button:disabled { opacity: 0.45; cursor: default; }
  .err { margin: 0; font-size: 13px; color: var(--del); }
</style>
