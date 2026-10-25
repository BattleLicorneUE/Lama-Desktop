<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { COMPACT_AT, estimateTokens, formatTokens, type ChatStore } from "$lib/chat.svelte";
  import Icon from "$lib/Icon.svelte";
  import { PERMISSION_MODES } from "$lib/code-options";

  let { chat }: { chat: ChatStore } = $props();

  type Report = { system: number; skills: number; tools: number; mcp: number };
  type Bg = { id: string; command: string; running: boolean; code: number | null; started_ms: number; tail: string };
  type Server = { name: string; tools: string[]; error: string | null };

  let report = $state<Report | null>(null);
  let servers = $state<Server[]>([]);
  let bg = $state<Bg[]>([]);
  let open = $state<Record<string, boolean>>({ env: true, ctx: true, use: true, bg: true, mcp: true, out: true, tasks: true, rules: false });

  // Bilan des modifications de la session : un élément par fichier, différences depuis l'état d'origine.
  type Change = { path: string; kind: string; diff: string | null };
  const KIND: Record<string, string> = { created: "créé", modified: "modifié", deleted: "supprimé", dir: "dossier", moved: "déplacé", trashed: "corbeille" };
  let changes = $state<Change[]>([]);
  let openDiff = $state("");
  $effect(() => {
    void chat.sessionChanges;
    void chat.busy;
    invoke<Change[]>("agent_changes").then((c) => (changes = c)).catch(() => (changes = []));
  });

  // Règles du projet : AGENTS.md à la racine, lu par l'agent à chaque tâche.
  let rules = $state("");
  let savedRules = $state("");
  let rulesMsg = $state("");
  $effect(() => {
    const ws = chat.workspace;
    if (!ws) return;
    invoke<string>("project_rules_get", { workspace: ws }).then((t) => (rules = savedRules = t)).catch(() => (rules = savedRules = ""));
  });
  async function saveRules() {
    try {
      await invoke("project_rules_set", { workspace: chat.workspace, text: rules });
      savedRules = rules;
      rulesMsg = "Enregistré : l'agent le lira à la prochaine tâche.";
    } catch (e) {
      rulesMsg = String(e);
    }
    setTimeout(() => (rulesMsg = ""), 3500);
  }

  // Le poids des outils et du prompt dépend du mode, de la recherche web et des skills activés.
  $effect(() => {
    const args = { workspace: chat.workspace || null, mode: chat.permission, web: chat.webSearch, ctx: chat.ctxLimit };
    void chat.skills;
    invoke<Report>("context_report", args).then((r) => (report = r)).catch(() => (report = null));
  });
  // L'état des connecteurs ne change que si l'utilisateur les recharge : une lecture à l'ouverture et au retour sur la fenêtre suffit.
  const loadServers = () => invoke<Server[]>("mcp_status").then((s) => (servers = s)).catch(() => (servers = []));
  const loadBg = () => invoke<Bg[]>("bg_list").then((l) => (bg = l)).catch(() => (bg = []));
  async function stopBg(id: string) {
    await invoke("bg_stop", { id }).catch(() => {});
    await loadBg();
  }
  async function clearBg() {
    await invoke("bg_clear").catch(() => {});
    await loadBg();
  }
  onMount(() => {
    loadBg();
    // Les commandes en arrière-plan évoluent sans événement : lecture légère toutes les 2 secondes.
    const timer = setInterval(loadBg, 2000);
    loadServers();
    window.addEventListener("focus", loadServers);
    // Les connecteurs démarrent en arrière-plan à l'ouverture du profil : on est prévenu quand ils sont prêts.
    const ready = listen("mcp:ready", loadServers);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", loadServers);
      ready.then((off) => off());
    };
  });

  const limit = $derived(chat.ctxLimit);
  const tok = (chars: number) => Math.ceil(chars / 3.5);
  const memoryTokens = $derived(estimateTokens(chat.memories.map((m) => m.text).join("\n")));
  const messageTokens = $derived(estimateTokens(chat.messages.map((m) => m.content).join("\n")));

  // Catégories estimées (caractères / 3,5) ; le total, lui, est celui que mesure Ollama.
  const parts = $derived.by(() => {
    const list = [
      { key: "msg", label: "Messages", n: messageTokens, color: "#4a8ff0" },
      { key: "sys", label: "Prompt système", n: tok(report?.system ?? 0), color: "#e0703a" },
      { key: "tools", label: "Outils de l'agent", n: tok(report?.tools ?? 0), color: "#9b7bd6" },
      { key: "mcp", label: "Outils MCP", n: tok(report?.mcp ?? 0), color: "#2fa876" },
      { key: "skills", label: "Skills", n: tok(report?.skills ?? 0), color: "#b08a3a" },
      { key: "mem", label: "Mémoire", n: memoryTokens, color: "#8a8a8a" },
    ];
    const known = list.reduce((a, p) => a + p.n, 0);
    if (chat.promptTokens > known) list.push({ key: "other", label: "Autres (résultats d'outils…)", n: chat.promptTokens - known, color: "#c9c9c9" });
    return list;
  });
  const used = $derived(chat.promptTokens || parts.reduce((a, p) => a + p.n, 0));
  const buffer = $derived(Math.round(limit * (1 - COMPACT_AT)));
  const free = $derived(Math.max(0, limit - used - buffer));
  const pct = (n: number) => (limit ? (n / limit) * 100 : 0);

  const tasks = $derived([...chat.messages].reverse().find((m) => m.tasks?.length)?.tasks ?? []);
  const folder = $derived(chat.workspace.split(/[\\/]/).filter(Boolean).pop() ?? "—");
  const toggle = (k: string) => (open[k] = !open[k]);
</script>

<div class="panel">
  <section>
    <button class="title" onclick={() => toggle("env")} aria-expanded={open.env}>Environnement <span class:flip={!open.env}><Icon name="chevron" size={12} /></span></button>
    {#if open.env}
      <ul class="kv">
        <li><span>Modèle</span><strong>{chat.model || "—"}</strong></li>
        <li><span>Dossier</span><strong title={chat.workspace}>{folder}</strong></li>
        <li><span>Mode</span><strong>{PERMISSION_MODES.find((m) => m.id === chat.permission)?.label ?? chat.permission}</strong></li>
        <li><span>Effort</span><strong>{chat.effort}</strong></li>
      </ul>
    {/if}
  </section>

  <section>
    <button class="title" onclick={() => toggle("ctx")} aria-expanded={open.ctx}>Contexte <span class:flip={!open.ctx}><Icon name="chevron" size={12} /></span></button>
    {#if open.ctx}
      <div class="total"><strong>{Math.round(pct(used))} % utilisé</strong><span>{formatTokens(used)} / {formatTokens(limit)}</span></div>
      <div class="bar" role="img" aria-label="Répartition du contexte">
        {#each parts as p (p.key)}{#if p.n > 0}<i style:width="{pct(p.n)}%" style:background={p.color}></i>{/if}{/each}
      </div>
      <ul class="legend">
        {#each parts as p (p.key)}
          <li><b style:background={p.color}></b><span>{p.label}</span><em>{formatTokens(p.n)}</em><em>{pct(p.n).toFixed(1)} %</em></li>
        {/each}
        <li class="dim"><b></b><span>Tampon avant compactage</span><em>{formatTokens(buffer)}</em><em>{pct(buffer).toFixed(1)} %</em></li>
        <li class="dim"><b></b><span>Espace libre</span><em>{formatTokens(free)}</em><em>{pct(free).toFixed(1)} %</em></li>
      </ul>
      <p class="note">Total mesuré par Ollama ; le détail est estimé (≈ 3,5 caractères par jeton).</p>
    {/if}
  </section>

  <section>
    <button class="title" onclick={() => toggle("use")} aria-expanded={open.use}>Consommation <span class:flip={!open.use}><Icon name="chevron" size={12} /></span></button>
    {#if open.use}
      <ul class="kv">
        <li><span>Aujourd'hui</span><strong>{formatTokens(chat.usage.dayIn + chat.usage.dayOut)} jetons</strong></li>
        <li><span>· envoyés / générés</span><strong>{formatTokens(chat.usage.dayIn)} / {formatTokens(chat.usage.dayOut)}</strong></li>
        <li><span>Total du profil</span><strong>{formatTokens(chat.usage.totalIn + chat.usage.totalOut)} jetons</strong></li>
      </ul>
      <p class="note">Mesuré d'après les réponses d'Ollama (sous-agents compris). Les limites de ton compte se consultent sur ollama.com.</p>
    {/if}
  </section>

  <section>
    <button class="title" onclick={() => toggle("bg")} aria-expanded={open.bg}>
      Tâches en arrière-plan{#if bg.some((b) => b.running)} ({bg.filter((b) => b.running).length}){/if} <span class:flip={!open.bg}><Icon name="chevron" size={12} /></span>
    </button>
    {#if open.bg}
      <ul class="plain bglist">
        {#each bg as b (b.id)}
          <li>
            <span class="dot" class:bad={!b.running && b.code !== 0}></span>
            <div class="grow">
              <code class="cmd" title={b.command}>{b.command}</code>
              {#if b.tail}<pre class="tail">{b.tail.split("\n").slice(-3).join("\n")}</pre>{/if}
            </div>
            {#if b.running}<button class="mini" onclick={() => stopBg(b.id)} aria-label="Arrêter {b.command}">Arrêter</button>{:else}<em>code {b.code}</em>{/if}
          </li>
        {:else}
          <li class="empty">Aucune commande en arrière-plan.</li>
        {/each}
      </ul>
      {#if bg.some((b) => !b.running)}<button class="mini wide" onclick={clearBg}>Effacer les terminées</button>{/if}
    {/if}
  </section>

  <section>
    <button class="title" onclick={() => toggle("mcp")} aria-expanded={open.mcp}>Connecteurs MCP <span class:flip={!open.mcp}><Icon name="chevron" size={12} /></span></button>
    {#if open.mcp}
      <ul class="plain">
        {#each servers as s (s.name)}
          <li>
            <span class="dot" class:bad={!!s.error}></span>
            <span class="grow" title={s.error ?? s.tools.join(", ")}>{s.name}</span>
            <em>{s.error ? "erreur" : `${s.tools.length} outil${s.tools.length > 1 ? "s" : ""}`}</em>
          </li>
        {:else}
          <li class="empty">Aucun serveur configuré (Réglages → Connecteurs).</li>
        {/each}
      </ul>
    {/if}
  </section>

  <section>
    <button class="title" onclick={() => toggle("out")} aria-expanded={open.out}>Résultats <span class:flip={!open.out}><Icon name="chevron" size={12} /></span></button>
    {#if open.out}
      <ul class="plain">
        {#each changes as f (f.path)}
          <li class="chg">
            <button class="chgrow" onclick={() => (openDiff = openDiff === f.path ? "" : f.path)} disabled={!f.diff} title={f.path}>
              <Icon name="file" size={14} /><span class="grow">{f.path.split(/[\\/]/).pop()}</span><em>{KIND[f.kind] ?? f.kind}</em>
            </button>
            {#if openDiff === f.path && f.diff}
              <pre class="diff">{#each f.diff.split("\n") as l}<span class:add={l.startsWith("+") && !l.startsWith("+++")} class:del={l.startsWith("-") && !l.startsWith("---")}>{l || " "}</span>{/each}</pre>
            {/if}
          </li>
        {:else}
          <li class="empty">Aucun fichier modifié dans cette conversation.</li>
        {/each}
      </ul>
    {/if}
  </section>

  <section>
    <button class="title" onclick={() => toggle("rules")} aria-expanded={open.rules}>Règles du projet <span class:flip={!open.rules}><Icon name="chevron" size={12} /></span></button>
    {#if open.rules}
      {#if chat.workspace}
        <p class="note">Fichier <code>AGENTS.md</code> à la racine du projet : l'agent le lit à chaque tâche.</p>
        <textarea bind:value={rules} rows="6" spellcheck="false" placeholder="Ex. TypeScript strict. Lancer npm test après chaque modification. Pas de nouvelle dépendance sans me demander."></textarea>
        <button class="mini wide" onclick={saveRules} disabled={rules === savedRules}>Enregistrer</button>
        {#if rulesMsg}<p class="note" role="status">{rulesMsg}</p>{/if}
      {:else}
        <p class="note">Choisis un dossier de travail.</p>
      {/if}
    {/if}
  </section>

  <section>
    <button class="title" onclick={() => toggle("tasks")} aria-expanded={open.tasks}>Tâches <span class:flip={!open.tasks}><Icon name="chevron" size={12} /></span></button>
    {#if open.tasks}
      <ul class="plain">
        {#each tasks as t}
          <li class:done={t.status === "completed"}>
            <span class="mark">{t.status === "completed" ? "✓" : t.status === "in_progress" ? "●" : "○"}</span>
            <span class="grow">{t.content}</span>
          </li>
        {:else}
          <li class="empty">{chat.busy ? "L'agent travaille…" : "Aucune tâche en cours."}</li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .panel { flex: 1; overflow-y: auto; padding: 8px; display: flex; flex-direction: column; gap: 8px; }
  section { border: 1px solid var(--border); border-radius: 12px; padding: 8px 12px 10px; background: var(--surface); }
  .title { width: 100%; display: flex; align-items: center; gap: 6px; border: 0; background: none; padding: 2px 0 6px; font-weight: 600; font-size: 13.5px; color: var(--fg); text-align: left; }
  .title span { display: inline-flex; transition: transform 0.15s; }
  .title span.flip { transform: rotate(-90deg); }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; font-size: 13px; }
  .kv li { display: flex; justify-content: space-between; gap: 10px; }
  .kv span { color: var(--muted); }
  .kv strong { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .total { display: flex; justify-content: space-between; font-size: 13px; margin-bottom: 6px; }
  .total span { color: var(--muted); }
  .bar { display: flex; height: 6px; border-radius: 3px; background: var(--hover); overflow: hidden; margin-bottom: 8px; }
  .bar i { display: block; min-width: 2px; }
  .legend li { display: grid; grid-template-columns: 10px 1fr auto 52px; align-items: center; gap: 8px; }
  .legend b { width: 10px; height: 10px; border-radius: 3px; background: var(--border); }
  .legend em { font-style: normal; color: var(--muted); text-align: right; font-variant-numeric: tabular-nums; }
  .legend .dim { color: var(--muted); }
  .note { margin: 8px 0 0; color: var(--muted); font-size: 12px; line-height: 1.4; }
  .plain li { display: flex; align-items: center; gap: 8px; }
  .plain .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .plain em { font-style: normal; color: var(--muted); font-size: 12px; }
  .empty { color: var(--muted); }
  .bglist li { align-items: flex-start; }
  .cmd { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; background: var(--code); padding: 1px 6px; border-radius: 5px; }
  .tail { margin: 4px 0 0; font-size: 11.5px; color: var(--muted); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 70px; overflow: hidden; font-family: ui-monospace, Consolas, monospace; }
  .mini { border: 1px solid var(--border); background: var(--surface); border-radius: 7px; padding: 2px 8px; font-size: 12px; flex: none; }
  .mini:hover { border-color: var(--accent); }
  .mini.wide { margin-top: 6px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--add); flex-shrink: 0; }
  .dot.bad { background: var(--del); }
  .mark { width: 14px; text-align: center; color: var(--accent); }
  .chg { display: block; }
  .chgrow { display: flex; align-items: center; gap: 8px; width: 100%; border: 0; background: none; padding: 0; text-align: left; font-size: 13px; color: var(--fg); }
  .chgrow:disabled { cursor: default; }
  .chgrow .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diff { margin: 4px 0 2px; max-height: 220px; overflow: auto; font-family: ui-monospace, Consolas, monospace; font-size: 11.5px; line-height: 1.45; background: var(--code); border-radius: 8px; padding: 6px 0; }
  .diff span { display: block; padding: 0 8px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .diff .add { background: var(--add-bg); color: var(--add); }
  .diff .del { background: var(--del-bg); color: var(--del); }
  textarea { width: 100%; box-sizing: border-box; border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 7px 9px; outline: none; font-size: 12.5px; line-height: 1.45; resize: vertical; font-family: ui-monospace, Consolas, monospace; }
  textarea:focus { border-color: var(--accent); }
  .done .grow { color: var(--muted); text-decoration: line-through; }
</style>
