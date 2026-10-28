<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { LOCAL_PREFIX, fmtBytes, type ChatStore } from "$lib/chat.svelte";
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
  type Status = { engine_bytes: number; partial_bytes: number; free_mb: number };

  let { chat, hardware }: { chat: ChatStore; hardware: Hardware } = $props();
  let items = $state<Item[]>([]);
  let error = $state("");
  let showHeavy = $state(false);
  let status = $state<Status>({ engine_bytes: 0, partial_bytes: 0, free_mb: 0 });
  let tab = $state<"installed" | "catalog" | null>(null);
  let filter = $state<"all" | "chat" | "code" | "reasoning" | "vision">("all");
  let openCtx = $state<string | null>(null);
  let removing = $state<string | null>(null);

  $effect(() => {
    invoke<Item[]>("local_models_assess", { hardware })
      .then((r) => (items = r))
      .catch((e) => (error = String(e)));
  });
  const refreshStatus = () => invoke<Status>("local_status").then((s) => (status = s)).catch(() => {});
  onMount(() => {
    void chat.loadLocal();
    void refreshStatus();
  });
  $effect(() => {
    // Après chaque installation, suppression ou nettoyage, l'espace pris a changé.
    void chat.localInstalled.length;
    void chat.localProgress?.id;
    void chat.localNotice;
    void refreshStatus();
  });
  // Pendant un téléchargement, l'espace libre baisse : on le relit de temps en temps.
  $effect(() => {
    if (!chat.localProgress) return;
    const t = setInterval(refreshStatus, 5000);
    return () => clearInterval(t);
  });

  const gb = (mb: number) => `${(mb / 1024).toFixed(1).replace(".", ",")} Go`;
  const USE: Record<string, string> = { chat: "discussion", code: "code", reasoning: "raisonnement" };
  const SPEED = { fast: "Rapide", medium: "Correct", slow: "Lent" } as const;
  const MODE = { gpu: "carte graphique", mixed: "carte graphique + mémoire du PC", cpu: "processeur" } as const;
  const FILTERS = [
    { id: "all", label: "Tous" },
    { id: "chat", label: "Discussion" },
    { id: "code", label: "Code" },
    { id: "reasoning", label: "Raisonnement" },
    { id: "vision", label: "Images" },
  ] as const;

  const installedIds = $derived(new Set(chat.localInstalled.map((m) => m.id)));
  const view = $derived(tab ?? (chat.localInstalled.length ? "installed" : "catalog"));
  const busy = $derived(chat.localProgress !== null);
  const nameOf = (id: string) => items.find((i) => i.id === id)?.name ?? chat.localInstalled.find((m) => m.id === id)?.name ?? id;

  const matches = (m: Item) => filter === "all" || (filter === "vision" ? m.vision : m.uses.includes(filter));
  const rank = (m: Item) => (m.best ? 0 : m.level === "recommended" ? 1 : 2);
  const catalog = $derived(items.filter((i) => !installedIds.has(i.id)).sort((a, b) => rank(a) - rank(b)));
  const available = $derived(catalog.filter((i) => i.level !== "heavy" && matches(i)));
  const heavy = $derived(catalog.filter((i) => i.level === "heavy" && matches(i)));
  const countFor = (f: (typeof FILTERS)[number]["id"]) => catalog.filter((i) => i.level !== "heavy" && (f === "all" || (f === "vision" ? i.vision : i.uses.includes(f)))).length;
  const starter = $derived(!chat.localInstalled.length && !busy ? catalog.find((i) => i.best) : undefined);

  const modelsBytes = $derived(chat.localInstalled.reduce((n, m) => n + m.size_bytes, 0));
  const usedBytes = $derived(modelsBytes + status.engine_bytes + status.partial_bytes);
  const freeBytes = $derived(status.free_mb * 1024 * 1024);
  const usedPct = $derived(usedBytes + freeBytes ? Math.max(2, Math.round((usedBytes / (usedBytes + freeBytes)) * 100)) : 0);

  const PHASE: Record<string, string> = { moteur: "Téléchargement du moteur", modèle: "Téléchargement du modèle", vérification: "Vérification du fichier", décompression: "Installation du moteur", préparation: "Préparation" };
  const pct = $derived(chat.localProgress && chat.localProgress.total ? Math.min(100, Math.round((chat.localProgress.done / chat.localProgress.total) * 100)) : 0);
  const eta = $derived.by(() => {
    const p = chat.localProgress;
    if (!p || !p.total || chat.localRate < 50_000 || (p.phase !== "modèle" && p.phase !== "moteur")) return "";
    const s = Math.max(1, Math.round((p.total - p.done) / chat.localRate));
    return s >= 90 ? `${Math.round(s / 60)} min restantes` : `${s} s restantes`;
  });
  const rate = $derived(chat.localRate >= 50_000 ? `${(chat.localRate / 1024 ** 2).toFixed(1).replace(".", ",")} Mo/s` : "");

  async function install(m: Item, force = false) {
    if (force && !(await askConfirm(`${m.name} est trop lourd pour ce PC : ${m.reason} Il sera très lent ou inutilisable. Installer quand même ?`, "Installer quand même"))) return;
    if (!m.disk_ok && !(await askConfirm("Il n'y a peut-être pas assez de place sur le disque pour ce modèle. Essayer quand même ?", "Essayer"))) return;
    chat.localNotice = "";
    tab = view;
    await chat.installLocal(m.id);
  }

  async function remove(id: string) {
    const m = chat.localInstalled.find((x) => x.id === id);
    if (!m) return;
    const current = chat.model === LOCAL_PREFIX + id;
    const text = `Supprimer ${m.name} ? Ça libère ${fmtBytes(m.size_bytes)} sur le disque. Tu pourras le télécharger de nouveau.${current ? " C'est le modèle choisi en ce moment : Lama en prendra un autre." : ""}`;
    if (!(await askConfirm(text, "Supprimer"))) return;
    removing = id;
    chat.localNotice = "";
    await chat.removeLocal(id);
    removing = null;
  }

  const tok = (n: number) => (n >= 1024 ? `${Math.round(n / 1024)} k` : String(n));
  const gbMem = (mb: number) => (mb >= 1024 ? `${(mb / 1024).toFixed(1).replace(".", ",")} Go` : `${mb} Mo`);
  const generating = (id: string) => chat.anyRunning && chat.model === LOCAL_PREFIX + id;

  async function pickCtx(id: string, value: string) {
    await chat.setLocalCtx(id, Number(value));
  }

  function use(id: string) {
    chat.setModel(LOCAL_PREFIX + id);
  }

  const queuePos = (id: string) => chat.localQueue.indexOf(id) + 1;

  // Le message de résultat s'efface tout seul.
  $effect(() => {
    if (!chat.localNotice) return;
    const t = setTimeout(() => (chat.localNotice = ""), 9000);
    return () => clearTimeout(t);
  });
</script>

<section>
  <h3>Modèles</h3>
  <p class="muted">Pas besoin d'internet ni de clé. Lama télécharge d'abord le moteur (environ 40 Mo, une seule fois), puis les modèles que tu choisis : tu peux en installer plusieurs, ils se téléchargent l'un après l'autre.
    Un seul est chargé en mémoire à la fois, libéré après 15 minutes sans servir.</p>

  <div class="disk" aria-label="Espace disque">
    <div class="dtop"><b>{fmtBytes(usedBytes)}</b> utilisés par Lama <span class="muted">· {status.free_mb ? `${fmtBytes(freeBytes)} libres sur le disque` : "espace libre inconnu"}</span></div>
    <div class="bar thin" role="img" aria-label="{usedPct} % de l'espace utilisable pris par Lama"><i style:width="{usedPct}%"></i></div>
    {#if status.partial_bytes > 0 && !busy}
      <div class="dfoot">
        <span class="muted">{fmtBytes(status.partial_bytes)} de téléchargements interrompus.</span>
        <button class="link" onclick={() => chat.cleanLocalPartials()}>Nettoyer</button>
      </div>
    {/if}
  </div>

  {#if chat.localNotice}
    <p class="ok" role="status">{chat.localNotice} <button class="x" aria-label="Fermer ce message" onclick={() => (chat.localNotice = "")}>×</button></p>
  {/if}
  {#if chat.localError}
    <p class="bad" role="alert">{chat.localError} <button class="x" aria-label="Fermer ce message" onclick={() => (chat.localError = "")}>×</button></p>
  {/if}
  {#if error}<p class="bad" role="alert">{error}</p>{/if}

  {#if chat.localProgress}
    <div class="progress" role="status">
      <div class="ptop">
        <b>{nameOf(chat.localProgress.id)}</b>
        <span class="muted">{PHASE[chat.localProgress.phase] ?? chat.localProgress.phase}{pct ? ` · ${pct} %` : ""}</span>
        <button onclick={() => chat.cancelLocal(chat.localProgress!.id)}>Annuler</button>
      </div>
      <div class="bar"><i style:width="{pct}%"></i></div>
      <p class="muted">
        {chat.localProgress.total ? `${fmtBytes(chat.localProgress.done)} sur ${fmtBytes(chat.localProgress.total)}` : "…"}{rate ? ` · ${rate}` : ""}{eta ? ` · ${eta}` : ""}
      </p>
      {#if chat.localQueue.length}
        <div class="queue">
          <span class="muted">Ensuite :</span>
          {#each chat.localQueue as id, i (id)}
            <span class="chip">{i + 1}. {nameOf(id)}<button class="x" aria-label="Retirer {nameOf(id)} de la file" onclick={() => chat.dequeueLocal(id)}>×</button></span>
          {/each}
          <button class="link" onclick={() => chat.cancelAllLocal()}>Tout annuler</button>
        </div>
      {/if}
      <p class="muted">Tu peux fermer cette page, le téléchargement continue. S'il s'arrête, il reprendra où il en était.</p>
    </div>
  {/if}

  <div class="tabs" role="tablist" aria-label="Modèles">
    <button role="tab" aria-selected={view === "installed"} class:on={view === "installed"} onclick={() => (tab = "installed")}>Installés <span class="count">{chat.localInstalled.length}</span></button>
    <button role="tab" aria-selected={view === "catalog"} class:on={view === "catalog"} onclick={() => (tab = "catalog")}>À installer <span class="count">{catalog.filter((i) => i.level !== "heavy").length}</span></button>
  </div>

  {#if view === "installed"}
    {#each chat.localInstalled as m (m.id)}
      {@const current = chat.model === LOCAL_PREFIX + m.id}
      <article class="have" class:current>
        <header>
          <b>{m.name}</b>
          {#if current}<span class="tag star">En cours d'utilisation</span>{/if}
          <span class="tag soft">{m.tools ? "outils de l'agent" : "sans outils de l'agent"}</span>
          {#if m.vision}<span class="tag soft">images</span>{/if}
          <span class="size">{fmtBytes(m.size_bytes)}</span>
        </header>
        <div class="row">
          <button class="go" onclick={() => use(m.id)} disabled={chat.busy || current}>{current ? "Choisi" : "Utiliser"}</button>
          <button onclick={() => (openCtx = openCtx === m.id ? null : m.id)} aria-expanded={openCtx === m.id}>Taille de conversation : {tok(m.ctx)} {openCtx === m.id ? "▴" : "▾"}</button>
          <button class="danger" onclick={() => remove(m.id)} disabled={busy || removing !== null || generating(m.id)} title={generating(m.id) ? "Une réponse est en cours avec ce modèle." : busy ? "Attends la fin de l'installation." : ""}>
            {removing === m.id ? "Suppression…" : "Supprimer"}
          </button>
        </div>
        {#if openCtx === m.id}
          <div class="ctxbox">
            <label class="ctx">
              <span>Taille de la conversation (contexte)</span>
              <select value={m.ctx_setting} onchange={(e) => pickCtx(m.id, e.currentTarget.value)} disabled={chat.busy}>
                <option value={0}>Automatique · {tok(m.auto_ctx)} jetons</option>
                {#each m.options as o (o.ctx)}
                  <option value={o.ctx}>{tok(o.ctx)} jetons · cache ≈ {gbMem(o.mem_mb)}</option>
                {/each}
              </select>
            </label>
            <p class="why">
              En ce moment : <b>{tok(m.ctx)} jetons</b>, soit environ {Math.round(m.ctx * 0.75).toLocaleString("fr-FR")} mots. Le modèle va jusqu'à {tok(m.max_ctx)}.
              Plus c'est grand, plus il faut de mémoire et plus la lecture d'un long texte est lente. Pour coder, vise 32 k au minimum. Le changement compte dès le prochain message.
            </p>
            <p class="why">Licence : {m.license}</p>
          </div>
        {/if}
      </article>
    {:else}
      <div class="empty">
        <p><b>Aucun modèle installé pour l'instant.</b></p>
        {#if starter}
          <p class="muted">Pour démarrer, Lama te conseille <b>{starter.name}</b> ({gb(starter.size_mb)}) : {starter.reason}</p>
          <div class="row"><button class="go" onclick={() => install(starter)}>Installer {starter.name}</button><button onclick={() => (tab = "catalog")}>Voir tous les modèles</button></div>
        {:else}
          <div class="row"><button onclick={() => (tab = "catalog")}>Voir les modèles disponibles</button></div>
        {/if}
      </div>
    {/each}
  {:else}
    <div class="chips" role="group" aria-label="Filtrer les modèles">
      {#each FILTERS as f (f.id)}
        <button class="fchip" class:on={filter === f.id} aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>{f.label} <span class="count">{countFor(f.id)}</span></button>
      {/each}
    </div>

    {#each available as m (m.id)}
      {@const installing = chat.localProgress?.id === m.id}
      {@const pos = queuePos(m.id)}
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
          {#if installing}
            <button disabled>Installation en cours…</button>
          {:else if pos}
            <button disabled>En attente · n° {pos}</button>
            <button onclick={() => chat.dequeueLocal(m.id)}>Retirer</button>
          {:else}
            <button class="go" onclick={() => install(m)}>{busy ? "Mettre en file" : "Installer"}</button>
          {/if}
          <span class="muted">Licence : {m.license}</span>
        </div>
      </article>
    {:else}
      <p class="muted">{!items.length ? "Analyse en cours…" : filter === "all" ? "Tous les modèles adaptés à ton PC sont déjà installés." : "Aucun modèle de cette catégorie à installer."}</p>
    {/each}

    {#if heavy.length}
      <button class="link" onclick={() => (showHeavy = !showHeavy)}>{showHeavy ? "Masquer" : "Voir"} {heavy.length === 1 ? "le modèle trop lourd" : `les ${heavy.length} modèles trop lourds`} pour ce PC</button>
      {#if showHeavy}
        {#each heavy as m (m.id)}
          <article class="off">
            <header><b>{m.name}</b><span class="size">{gb(m.size_mb)}</span></header>
            <p class="why">{m.reason}</p>
            <div class="row"><button onclick={() => install(m, true)} disabled={chat.localProgress?.id === m.id || queuePos(m.id) > 0}>Installer quand même…</button></div>
          </article>
        {/each}
      {/if}
    {/if}
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; margin-top: 28px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .bad, .ok { margin: 0; font-size: 13.5px; word-break: break-word; white-space: pre-wrap; display: flex; gap: 8px; align-items: flex-start; }
  .bad { color: var(--del); }
  .ok { color: var(--add); }
  .x { border: none; background: none; color: var(--muted); font-size: 16px; line-height: 1; padding: 0 4px; margin-left: auto; cursor: pointer; }
  article { border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  article.best, article.current { border-color: var(--fg); }
  article.off { opacity: 0.75; }
  header { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .size { margin-left: auto; color: var(--muted); font-size: 13px; }
  .blurb { margin: 0; font-size: 13.5px; line-height: 1.45; }
  .why { margin: 0; font-size: 12.5px; color: var(--muted); line-height: 1.4; }
  .meta, .row { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .row { margin-top: 2px; gap: 8px; }
  .ctxbox { display: flex; flex-direction: column; gap: 6px; padding-top: 8px; border-top: 1px solid var(--border); }
  .ctx { display: flex; flex-direction: column; gap: 4px; font-size: 12.5px; color: var(--muted); max-width: 360px; }
  .ctx select { border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 8px; padding: 5px 8px; font: inherit; font-size: 13px; }
  .tag { font-size: 11.5px; padding: 2px 8px; border-radius: 999px; border: 1px solid var(--border); background: var(--code); }
  .tag.soft { color: var(--muted); }
  .tag.no { opacity: 0.7; text-decoration: line-through; text-decoration-thickness: 1px; }
  .tag.fast { color: var(--add); border-color: color-mix(in srgb, var(--add) 50%, var(--border)); }
  .tag.slow { color: var(--del); border-color: color-mix(in srgb, var(--del) 50%, var(--border)); }
  .tag.star { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 5px 12px; color: var(--fg); font: inherit; font-size: 13px; cursor: pointer; }
  button:disabled { opacity: 0.45; cursor: default; }
  button.go { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  button.danger:not(:disabled):hover { color: var(--del); border-color: var(--del); }
  .link { align-self: flex-start; border: none; background: none; color: var(--muted); text-decoration: underline; padding: 2px 0; }
  .disk { border: 1px solid var(--border); border-radius: 12px; padding: 10px 14px; display: flex; flex-direction: column; gap: 6px; font-size: 13.5px; }
  .dfoot { display: flex; gap: 8px; align-items: center; }
  .progress { border: 1px solid var(--fg); border-radius: 12px; padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  .ptop { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .ptop button { margin-left: auto; }
  .queue { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .chip { display: inline-flex; align-items: center; gap: 2px; font-size: 12.5px; padding: 2px 4px 2px 10px; border-radius: 999px; border: 1px solid var(--border); background: var(--code); }
  .chip .x { margin-left: 0; }
  .bar { height: 6px; border-radius: 3px; background: var(--code); overflow: hidden; }
  .bar.thin { height: 4px; }
  .bar i { display: block; height: 100%; background: var(--fg); transition: width 0.25s; }
  .tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border); margin-top: 6px; }
  .tabs button { border: none; background: none; border-radius: 8px 8px 0 0; padding: 7px 14px; color: var(--muted); border-bottom: 2px solid transparent; font-size: 14px; }
  .tabs button.on { color: var(--fg); border-bottom-color: var(--fg); font-weight: 600; }
  .count { font-size: 11.5px; color: var(--muted); font-weight: 400; margin-left: 2px; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .fchip { border-radius: 999px; padding: 3px 12px; font-size: 12.5px; }
  .fchip.on { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  .fchip.on .count { color: inherit; opacity: 0.8; }
  .empty { border: 1px dashed var(--border); border-radius: 12px; padding: 16px; display: flex; flex-direction: column; gap: 8px; font-size: 14px; }
  .empty p { margin: 0; }
</style>
