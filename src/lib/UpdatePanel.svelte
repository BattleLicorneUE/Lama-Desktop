<script lang="ts">
  import { onMount } from "svelte";
  import type { ChatStore } from "$lib/chat.svelte";
  import { updater } from "$lib/updater.svelte";

  let { chat }: { chat: ChatStore } = $props();

  onMount(() => {
    if (updater.status === "idle") void updater.check();
  });

  const pct = $derived(updater.total ? Math.min(100, Math.round((updater.done / updater.total) * 100)) : 0);
</script>

<section>
  <h3>Mises à jour</h3>
  <p class="muted">
    Lama Desktop vérifie au lancement s'il existe une nouvelle version publiée sur GitHub. Chaque mise à jour est signée : l'application refuse d'installer un fichier qui ne vient pas de son auteur.
    Tes conversations, ton coffre et tes réglages ne sont pas touchés.
  </p>
  <p class="ver">Version installée : <b>{updater.current || "…"}</b></p>

  {#if updater.status === "checking"}
    <p class="muted">Recherche en cours…</p>
  {:else if updater.status === "none"}
    <p class="ok" role="status">Tu as la dernière version.</p>
  {:else if updater.status === "available" || updater.status === "installing"}
    <div class="card">
      <b>Version {updater.version} disponible</b>
      {#if updater.notes}<pre>{updater.notes}</pre>{/if}
      {#if updater.status === "installing"}
        <div class="bar"><i style:width="{pct}%"></i></div>
        <p class="muted">Téléchargement{pct ? ` ${pct} %` : ""}… L'application va se fermer et se relancer toute seule.</p>
      {:else}
        {#if chat.anyRunning}<p class="muted">Une réponse est en cours : attends sa fin avant d'installer.</p>{/if}
        <div class="row"><button class="go" onclick={() => updater.install()} disabled={chat.anyRunning}>Installer et redémarrer</button></div>
      {/if}
    </div>
  {:else if updater.status === "error"}
    <p class="bad" role="alert">La vérification a échoué : {updater.error}</p>
  {/if}

  <div class="row">
    <button onclick={() => updater.check()} disabled={updater.status === "checking" || updater.status === "installing"}>Rechercher une mise à jour</button>
  </div>
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .ver { margin: 0; font-size: 14px; }
  .ok { margin: 0; font-size: 13.5px; color: var(--add); }
  .bad { margin: 0; font-size: 13.5px; color: var(--del); word-break: break-word; }
  .row { display: flex; gap: 8px; flex-wrap: wrap; }
  .card { border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 14px; display: flex; flex-direction: column; gap: 10px; }
  pre { margin: 0; white-space: pre-wrap; font: inherit; font-size: 13px; color: var(--muted); max-height: 180px; overflow: auto; }
  .bar { height: 6px; border-radius: 3px; background: var(--code); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--fg); transition: width 0.2s; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); font: inherit; }
  button:disabled { opacity: 0.45; }
  button.go { background: var(--fg); color: var(--bg); border-color: var(--fg); }
</style>
