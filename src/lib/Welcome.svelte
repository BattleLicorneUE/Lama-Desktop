<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { ChatStore } from "$lib/chat.svelte";

  let { chat }: { chat: ChatStore } = $props();

  let key = $state("");
  let connecting = $state(false);

  async function connect() {
    connecting = true;
    await chat.connectCloud(key);
    connecting = false;
    if (chat.cloudStatus !== "error") key = "";
  }
</script>

<div class="card" role="region" aria-label="Premiers pas">
  <h2>Connecte-toi à Ollama Cloud</h2>
  <p>Cette application utilise uniquement l'API d'Ollama : les modèles tournent chez Ollama, rien n'est installé sur ton PC.</p>
  <ol>
    <li>Crée une clé API sur
      <button class="link" onclick={() => openUrl("https://ollama.com/settings/keys").catch(() => {})}>ollama.com/settings/keys</button>.
    </li>
    <li>Colle-la ci-dessous, puis clique sur « Connecter ».</li>
  </ol>
  <form class="row" onsubmit={(e) => { e.preventDefault(); if (key.trim()) connect(); }}>
    <input type="password" bind:value={key} placeholder="Clé API Ollama" autocomplete="off" spellcheck="false" aria-label="Clé API Ollama" />
    <button class="primary" disabled={!key.trim() || connecting}>{connecting ? "Test…" : "Connecter"}</button>
  </form>
  {#if chat.cloudStatus === "error"}<p class="err">{chat.cloudError}</p>{/if}
  <p class="small">Tes messages sont envoyés à ollama.com et les limites d'usage de ton compte s'appliquent. La clé reste sur ce PC.</p>
</div>

<style>
  .card { width: 100%; text-align: left; background: var(--surface); border: 1px solid var(--border); border-radius: 18px; padding: 22px 24px; display: flex; flex-direction: column; gap: 12px; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 22px; }
  p { margin: 0; color: var(--muted); line-height: 1.5; }
  ol { margin: 0; padding-left: 20px; color: var(--muted); line-height: 1.6; }
  .small { font-size: 13px; }
  .row { display: flex; gap: 8px; }
  input { flex: 1; min-width: 200px; border: 1px solid var(--border); background: var(--bg); border-radius: 10px; padding: 8px 12px; outline: none; }
  input:focus { border-color: var(--accent); }
  button { border: 1px solid var(--border); background: var(--bg); border-radius: 10px; padding: 8px 16px; }
  button:hover:not(:disabled) { border-color: var(--accent); }
  button:disabled { opacity: 0.55; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .link { border: 0; background: none; color: var(--accent); padding: 0; text-decoration: underline; }
  .err { color: var(--del); font-size: 13px; margin: 0; }
</style>
