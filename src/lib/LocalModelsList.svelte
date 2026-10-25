<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { LOCAL_PREFIX, type ChatStore } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";

  type Hardware = Record<string, unknown>;
  type Item = {
    id: string;
    name: string;
    params_b: number;
    size_mb: number;
    uses: string[];
    tools: boolean;
    vision: boolean;
    blurb: string;
    license: string;
    level: "recommended" | "possible" | "heavy";
    mode: "gpu" | "mixed" | "cpu" | null;
    speed: "fast" | "medium" | "slow" | null;
    reason: string;
    disk_ok: boolean;
    need_mb: number;
    best: boolean;
  };

  let { chat, hardware }: { chat: ChatStore; hardware: Hardware } = $props();
  let items = $state<Item[]>([]);
  let error = $state("");
  let showHeavy = $state(false);
  let engineBytes = $state(0);

  $effect(() => {
    invoke<Item[]>("local_models_assess", { hardware })
      .then((r) => (items = r))
      .catch((e) => (error = String(e)));
  });
  const refreshEngine = () => invoke<{ engine_bytes: number }>("local_status").then((s) => (engineBytes = s.engine_bytes)).catch(() => {});
  onMount(() => {
    void chat.loadLocal();
    void refreshEngine();
  });
  $effect(() => {
    // Après chaque installation ou suppression, l'espace du moteur peut avoir changé.
    void chat.localInstalled.length;
    void refreshEngine();
  });

  const gb = (mb: number) => `${(mb / 1024).toFixed(1).replace(".", ",")} Go`;
  const bytes = (b: number) => (b >= 1024 ** 3 ? `${(b / 1024 ** 3).toFixed(1).replace(".", ",")} Go` : `${Math.round(b / 1024 ** 2)} Mo`);
  const USE: Record<string, string> = { chat: "discussion", code: "code", reasoning: "raisonnement" };
  const SPEED = { fast: "Rapide", medium: "Correct", slow: "Lent" } as const;
  const MODE = { gpu: "carte graphique", mixed: "carte graphique + mémoire du PC", cpu: "processeur" } as const;

  const installedIds = $derived(new Set(chat.localInstalled.map((m) => m.id)));
  const installedSize = $derived(chat.localInstalled.reduce((n, m) => n + m.size_bytes, 0) + engineBytes);
  const busy = $derived(chat.localProgress !== null);
  const available = $derived(items.filter((i) => i.level !== "heavy" && !installedIds.has(i.id)));
  const heavy = $derived(items.filter((i) => i.level === "heavy" && !installedIds.has(i.id)));
  const nameOf = (id: string) => items.find((i) => i.id === id)?.name ?? id;

  const PHASE: Record<string, string> = { moteur: "Téléchargement du moteur", modèle: "Téléchargement du modèle", vérification: "Vérification du fichier", décompression: "Installation du moteur", préparation: "Préparation" };
  const pct = $derived(chat.localProgress && chat.localProgress.total ? Math.min(100, Math.round((chat.localProgress.done / chat.localProgress.total) * 100)) : 0);

  async function install(m: Item, force = false) {
    if (force && !(await askConfirm(`${m.name} est trop lourd pour ce PC : ${m.reason} Il sera très lent ou inutilisable. Installer quand même ?`, "Installer quand même"))) return;
    if (!m.disk_ok && !(await askConfirm("Il n'y a peut-être pas assez de place sur le disque pour ce modèle. Essayer quand même ?", "Essayer"))) return;
    await chat.installLocal(m.id);
  }

  async function remove(id: string) {
    if (await askConfirm(`Supprimer ${nameOf(id)} de ce PC ? Tu pourras le télécharger de nouveau plus tard.`, "Supprimer")) await chat.removeLocal(id);
  }

  function use(id: string) {
    chat.setModel(LOCAL_PREFIX + id);
  }
</script>

<section>
  <h3>Modèles locaux</h3>
  <p class="muted">
    Des modèles qui tournent sur ton ordinateur, sans internet ni clé API, avec le moteur llama.cpp téléchargé par l'application (environ 40 Mo, une seule fois).
    Un seul modèle est chargé à la fois ; il est déchargé de la mémoire après 15 minutes sans utilisation. Les tailles et vitesses sont approximatives.
  </p>

  {#if chat.localError}<p class="bad" role="alert">{chat.localError}</p>{/if}
  {#if error}<p class="bad" role="alert">{error}</p>{/if}

  {#if chat.localProgress}
    <div class="progress" role="status">
      <div class="ptop">
        <b>{nameOf(chat.localProgress.id)}</b>
        <span class="muted">{PHASE[chat.localProgress.phase] ?? chat.localProgress.phase}{pct ? ` · ${pct} %` : ""}{chat.localProgress.total ? ` · ${bytes(chat.localProgress.done)} / ${bytes(chat.localProgress.total)}` : ""}</span>
        <button onclick={() => chat.cancelLocal(chat.localProgress!.id)}>Annuler</button>
      </div>
      <div class="bar"><i style:width="{pct}%"></i></div>
      <p class="muted">Tu peux fermer cette page : le téléchargement continue. S'il est interrompu, il reprend là où il s'était arrêté.</p>
    </div>
  {/if}

  {#if chat.localInstalled.length}
    <h4>Installés <span class="muted">· {bytes(installedSize)} sur le disque, moteur compris</span></h4>
    {#each chat.localInstalled as m (m.id)}
      <article class="have">
        <header>
          <b>{m.name}</b>
          {#if chat.model === LOCAL_PREFIX + m.id}<span class="tag star">En cours</span>{/if}
          <span class="tag soft">{m.tools ? "outils de l'agent" : "sans outils de l'agent"}</span>
          {#if m.vision}<span class="tag soft">images</span>{/if}
          <span class="size">{bytes(m.size_bytes)}</span>
        </header>
        <div class="row">
          <button class="go" onclick={() => use(m.id)} disabled={chat.busy || chat.model === LOCAL_PREFIX + m.id}>Utiliser</button>
          <button onclick={() => remove(m.id)} disabled={busy}>Supprimer</button>
          <span class="muted">Licence : {m.license}</span>
        </div>
      </article>
    {/each}
  {/if}

  <h4>À installer</h4>
  {#each available as m (m.id)}
    <article class:best={m.best}>
      <header>
        <b>{m.name}</b>
        {#if m.best}<span class="tag star">Conseillé pour ton PC</span>{/if}
        {#if m.speed}<span class="tag" class:fast={m.speed === "fast"} class:slow={m.speed === "slow"}>{SPEED[m.speed]}</span>{/if}
        <span class="size">{gb(m.size_mb)}</span>
      </header>
      <p class="blurb">{m.blurb}</p>
      <div class="meta">
        {#each m.uses as u}<span class="tag soft">{USE[u] ?? u}</span>{/each}
        {#if m.vision}<span class="tag soft">images</span>{/if}
        <span class="tag soft" class:no={!m.tools}>{m.tools ? "outils de l'agent" : "sans outils de l'agent"}</span>
      </div>
      <p class="why">{m.mode ? `Sur : ${MODE[m.mode]}. ` : ""}{m.reason}{m.disk_ok ? "" : " Pas assez de place sur le disque."}</p>
      <div class="row">
        <button class="go" onclick={() => install(m)} disabled={busy}>Installer</button>
        <span class="muted">Licence : {m.license}</span>
      </div>
    </article>
  {:else}
    <p class="muted">{items.length ? "Tous les modèles adaptés à ce PC sont déjà installés." : "Analyse en cours…"}</p>
  {/each}

  {#if heavy.length}
    <button class="link" onclick={() => (showHeavy = !showHeavy)}>{showHeavy ? "Masquer" : "Voir"} les {heavy.length} modèles trop lourds pour ce PC</button>
    {#if showHeavy}
      {#each heavy as m (m.id)}
        <article class="off">
          <header><b>{m.name}</b><span class="size">{gb(m.size_mb)}</span></header>
          <p class="why">{m.reason}</p>
          <div class="row"><button onclick={() => install(m, true)} disabled={busy}>Installer quand même…</button></div>
        </article>
      {/each}
    {/if}
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; margin-top: 28px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  h4 { margin: 10px 0 0; font-size: 14px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .bad { margin: 0; font-size: 13.5px; color: var(--del); word-break: break-word; white-space: pre-wrap; }
  article { border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 12px 14px; display: flex; flex-direction: column; gap: 6px; }
  article.best { border-color: var(--fg); }
  article.off { opacity: 0.75; }
  header { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .size { margin-left: auto; color: var(--muted); font-size: 13px; }
  .blurb { margin: 0; font-size: 13.5px; line-height: 1.45; }
  .why { margin: 0; font-size: 12.5px; color: var(--muted); line-height: 1.4; }
  .meta, .row { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .row { margin-top: 2px; gap: 8px; }
  .tag { font-size: 11.5px; padding: 2px 8px; border-radius: 999px; border: 1px solid var(--border); background: var(--code); }
  .tag.soft { color: var(--muted); }
  .tag.no { opacity: 0.7; text-decoration: line-through; text-decoration-thickness: 1px; }
  .tag.fast { color: var(--add); border-color: color-mix(in srgb, var(--add) 50%, var(--border)); }
  .tag.slow { color: var(--del); border-color: color-mix(in srgb, var(--del) 50%, var(--border)); }
  .tag.star { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 5px 12px; color: var(--fg); font: inherit; font-size: 13px; cursor: pointer; }
  button:disabled { opacity: 0.45; cursor: default; }
  button.go { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  .link { align-self: flex-start; border: none; background: none; color: var(--muted); text-decoration: underline; padding: 2px 0; }
  .progress { border: 1px solid var(--fg); border-radius: 12px; padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  .ptop { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .ptop button { margin-left: auto; }
  .bar { height: 6px; border-radius: 3px; background: var(--code); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--fg); transition: width 0.25s; }
</style>
