<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";

  type Bg = { id: string; command: string; running: boolean; tail: string };

  let url = $state("");
  let shown = $state("");
  let runs = $state(0);
  let found = $state("");

  /** Adresse locale annoncée par un serveur de développement lancé en arrière-plan (la plus récente). */
  async function detect() {
    try {
      const list = await invoke<Bg[]>("bg_list");
      for (const b of [...list].reverse()) {
        if (!b.running) continue;
        const m = /https?:\/\/(?:localhost|127\.0\.0\.1|\[::1\]):\d+[^\s"')]*/.exec(b.tail);
        if (m) {
          found = m[0].replace(/\/$/, "");
          if (!url) url = found;
          return;
        }
      }
      found = "";
    } catch {
      found = "";
    }
  }

  onMount(() => {
    detect();
    const t = setInterval(detect, 3000);
    return () => clearInterval(t);
  });

  /** Seules les adresses locales sont affichées ici : l'agent travaille sur ton projet, pas sur Internet. */
  function local(u: string): string {
    const t = u.trim();
    if (!t) return "";
    const full = /^https?:\/\//i.test(t) ? t : `http://${t}`;
    try {
      const h = new URL(full).hostname;
      return h === "localhost" || h === "127.0.0.1" || h === "[::1]" ? full : "";
    } catch {
      return "";
    }
  }

  function go(e: Event) {
    e.preventDefault();
    shown = local(url);
    runs++;
  }
</script>

<div class="wrap">
  <form onsubmit={go}>
    <input bind:value={url} placeholder="http://localhost:5173" spellcheck="false" aria-label="Adresse locale" />
    <button>Afficher</button>
    {#if shown}<button type="button" title="Recharger" onclick={() => runs++}>↻</button>{/if}
  </form>
  {#if found && found !== shown}
    <p class="hint">Serveur détecté : <button class="link" onclick={() => { url = found; shown = found; runs++; }}>{found}</button></p>
  {/if}
  {#if shown}
    {#key runs}<iframe title="Aperçu de l'application" src={shown}></iframe>{/key}
    <button class="ext" onclick={() => openUrl(shown).catch(() => {})}>Ouvrir dans le navigateur</button>
  {:else}
    <p class="hint">Demande à l'agent de lancer le serveur de développement en arrière-plan : son adresse apparaît ici, et la page s'affiche à côté de la conversation. Seules les adresses locales (localhost) sont acceptées.</p>
  {/if}
</div>

<style>
  .wrap { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  form { display: flex; gap: 4px; padding: 8px; border-bottom: 1px solid var(--border); }
  input { flex: 1; min-width: 0; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 5px 9px; outline: none; font-size: 13px; }
  input:focus { border-color: var(--accent); }
  form button, .ext { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 4px 9px; font-size: 13px; }
  .hint { color: var(--muted); font-size: 13px; line-height: 1.5; padding: 8px 12px; margin: 0; }
  .link { border: 0; background: none; color: var(--accent); text-decoration: underline; padding: 0; }
  iframe { flex: 1; border: 0; background: white; min-height: 0; }
  .ext { margin: 6px 8px; align-self: flex-start; }
</style>
