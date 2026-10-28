<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { LOCAL_PREFIX, fmtBytes, type ChatStore } from "$lib/chat.svelte";

  let { chat }: { chat: ChatStore } = $props();

  type Pick = { id: string; name: string; size_mb: number; blurb: string; reason: string; level: "recommended" | "possible" | "heavy"; best: boolean; disk_ok: boolean; speed: "fast" | "medium" | "slow" | null };

  let key = $state("");
  let connecting = $state(false);
  let analysing = $state(true);
  let pick = $state<Pick | null>(null);
  let started = $state(false);

  onMount(async () => {
    void chat.loadLocal();
    try {
      const hardware = await invoke("hardware_detect");
      const items = await invoke<Pick[]>("local_models_assess", { hardware });
      // Le plus sûr pour une première fois : le modèle conseillé, sinon le plus petit qui reste possible.
      pick = items.find((i) => i.best) ?? items.find((i) => i.level === "recommended") ?? [...items].filter((i) => i.level === "possible").sort((a, b) => a.size_mb - b.size_mb)[0] ?? null;
    } catch {
      pick = null;
    }
    analysing = false;
  });

  // Dès que le modèle est installé, on le choisit : la première question peut être posée tout de suite.
  $effect(() => {
    if (started && pick && chat.localInstalled.some((m) => m.id === pick!.id) && chat.model !== LOCAL_PREFIX + pick.id) chat.setModel(LOCAL_PREFIX + pick.id);
  });

  const installing = $derived(!!pick && chat.localProgress?.id === pick.id);
  const pct = $derived(chat.localProgress && chat.localProgress.total ? Math.min(100, Math.round((chat.localProgress.done / chat.localProgress.total) * 100)) : 0);
  const rate = $derived(chat.localRate >= 50_000 ? `${(chat.localRate / 1024 ** 2).toFixed(1).replace(".", ",")} Mo/s` : "");
  const PHASE: Record<string, string> = { moteur: "Téléchargement du moteur", modèle: "Téléchargement du modèle", vérification: "Vérification du fichier", décompression: "Installation du moteur", préparation: "Préparation" };

  async function connect() {
    connecting = true;
    await chat.connectCloud(key);
    connecting = false;
    if (chat.cloudStatus !== "error") key = "";
  }

  async function installLocal() {
    if (!pick) return;
    started = true;
    chat.localError = "";
    await chat.installLocal(pick.id);
  }
</script>

<div class="card" role="region" aria-label="Premiers pas">
  <h2>Comment veux-tu faire tourner l'IA ?</h2>
  <p>Tu peux changer d'avis plus tard dans les Réglages, et utiliser les deux.</p>

  <section class="opt" aria-label="Sur ton PC">
    <h3>Sur ton PC <span class="tag">sans compte</span></h3>
    {#if analysing}
      <p class="small">Analyse de ton PC pour choisir un modèle qui lui convient…</p>
    {:else if pick}
      <p>Lama a regardé ton ordinateur et te conseille <b>{pick.name}</b> ({fmtBytes(pick.size_mb * 1024 * 1024)}). {pick.reason}</p>
      <p class="small">Gratuit, sans internet une fois installé, et tout reste sur ton PC. Le téléchargement se fait une seule fois.</p>
      {#if installing && chat.localProgress}
        <div class="progress" role="status">
          <div class="bar"><i style:width="{pct}%"></i></div>
          <p class="small">{PHASE[chat.localProgress.phase] ?? chat.localProgress.phase}{pct ? ` · ${pct} %` : ""}{rate ? ` · ${rate}` : ""}</p>
          <button onclick={() => chat.cancelLocal(pick!.id)}>Annuler</button>
        </div>
      {:else}
        <div class="row">
          <button class="primary" onclick={installLocal} disabled={!!chat.localProgress}>Installer {pick.name} et commencer</button>
        </div>
      {/if}
      {#if chat.localError}<p class="err" role="alert">{chat.localError}</p>{/if}
    {:else}
      <p class="small">Ce PC est trop juste pour faire tourner une IA correctement. Ollama Cloud, ci-dessous, sera bien plus confortable.</p>
    {/if}
  </section>

  <section class="opt" aria-label="Ollama Cloud">
    <h3>Avec Ollama Cloud <span class="tag">compte gratuit</span></h3>
    <p>De très gros modèles, rapides, sans rien installer. Tes messages sont envoyés à ollama.com et les limites d'usage de ton compte s'appliquent.</p>
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
    <p class="small">La clé reste sur ce PC.</p>
  </section>
</div>

<style>
  .card { width: 100%; text-align: left; background: var(--surface); border: 1px solid var(--border); border-radius: 18px; padding: 22px 24px; display: flex; flex-direction: column; gap: 12px; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 22px; }
  h3 { margin: 0; font-size: 15px; display: flex; align-items: center; gap: 8px; }
  .tag { font-size: 12px; font-weight: 400; color: var(--muted); border: 1px solid var(--border); border-radius: 999px; padding: 1px 8px; }
  p { margin: 0; color: var(--muted); line-height: 1.5; }
  p b { color: var(--fg); }
  ol { margin: 0; padding-left: 20px; color: var(--muted); line-height: 1.6; }
  .small { font-size: 13px; }
  .opt { border: 1px solid var(--border); background: var(--bg); border-radius: 14px; padding: 14px 16px; display: flex; flex-direction: column; gap: 8px; }
  .row { display: flex; gap: 8px; flex-wrap: wrap; }
  .progress { display: flex; flex-direction: column; gap: 6px; align-items: flex-start; }
  .bar { height: 6px; width: 100%; border-radius: 3px; background: var(--code); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--fg); transition: width 0.25s; }
  input { flex: 1; min-width: 200px; border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 8px 12px; outline: none; }
  input:focus { border-color: var(--accent); }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 8px 16px; }
  button:hover:not(:disabled) { border-color: var(--accent); }
  button:disabled { opacity: 0.55; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .link { border: 0; background: none; color: var(--accent); padding: 0; text-decoration: underline; }
  .err { color: var(--del); font-size: 13px; margin: 0; }
</style>
