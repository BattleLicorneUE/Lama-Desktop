<script lang="ts">
  import { enhance, renderMarkdown } from "$lib/markdown";
  import { isWrite, type ChatStore, type Msg } from "$lib/chat.svelte";
  import Icon from "$lib/Icon.svelte";
  import { normLang } from "$lib/artifacts";
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import { canSpeak, speak, stopSpeaking } from "$lib/speech";
  import Logo from "$lib/Logo.svelte";
  import ToolCard from "$lib/ToolCard.svelte";

  let { msg, index, streaming, last, chat }: {
    msg: Msg;
    index: number;
    streaming: boolean;
    last: boolean;
    chat: ChatStore;
  } = $props();

  // La réponse est le texte de la dernière étape ; les commentaires des étapes précédentes sont rangés avec le détail des actions.
  // Si la dernière étape n'a rien écrit, le dernier commentaire tient lieu de réponse.
  const answer = $derived(msg.content.trim() ? msg.content : !streaming && msg.notes?.length ? msg.notes[msg.notes.length - 1].text : "");
  const notes = $derived(answer && !msg.content.trim() ? (msg.notes ?? []).slice(0, -1) : (msg.notes ?? []));
  const html = $derived(msg.role === "assistant" ? renderMarkdown(answer) : "");
  let editing = $state(false);
  let draft = $state("");
  let copied = $state(false);

  function startEdit() {
    draft = msg.content;
    editing = true;
  }

  async function saveEdit() {
    editing = false;
    await chat.editMessage(index, draft);
  }

  let speaking = $state(false);
  function toggleSpeak() {
    if (speaking) {
      stopSpeaking();
      speaking = false;
    } else {
      speaking = true;
      speak(answer || msg.content, () => (speaking = false));
    }
  }
  onDestroy(() => speaking && stopSpeaking());

  async function copy() {
    await navigator.clipboard.writeText(answer || msg.content);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  const written = $derived(
    [...new Set((msg.tools ?? []).filter((t) => isWrite(t.name) && t.state === "done").map((t) => String(t.args.path ?? t.args.to)))],
  );
  const undoable = $derived(
    last && !streaming && chat.sessionChanges > 0
      ? (msg.tools ?? []).filter((t) => isWrite(t.name) && t.state === "done").length
      : 0,
  );

  // Les actions terminées se replient en une ligne (« Exécuté 2 commandes, créé 1 fichier… ») ; > pour le détail.
  // Celles qui attendent une réponse ou tournent encore restent toujours visibles.
  let showAll = $state(false);
  const settled = $derived((msg.tools ?? []).filter((t) => ["done", "undone", "error", "denied"].includes(t.state)));
  const collapsed = $derived(settled.length > 0 && !showAll);
  const shownTools = $derived(collapsed ? (msg.tools ?? []).filter((t) => !settled.includes(t)) : (msg.tools ?? []));
  // Déplié : les commentaires reprennent leur place entre les actions.
  const timeline = $derived.by(() => {
    const out: ({ note: string } | { tool: NonNullable<Msg["tools"]>[number] })[] = [];
    if (collapsed) return shownTools.map((tool) => ({ tool }));
    let n = 0;
    shownTools.forEach((tool, i) => {
      while (n < notes.length && notes[n].at <= i) out.push({ note: notes[n++].text });
      out.push({ tool });
    });
    while (n < notes.length) out.push({ note: notes[n++].text });
    return out;
  });
  const plural = (n: number, one: string, many = one + "s") => `${n} ${n > 1 ? many : one}`;
  const stats = $derived.by(() => {
    const paths = (name: string) => new Set(settled.filter((t) => t.name === name).map((t) => String(t.args.path ?? t.args.from ?? t.call_id)));
    const count = (pred: (n: string) => boolean) => settled.filter((t) => pred(t.name)).length;
    const parts: string[] = [];
    const cmds = count((n) => n === "run_command" || n === "verify");
    if (cmds) parts.push(`exécuté ${plural(cmds, "commande")}`);
    const reads = paths("read_file").size;
    if (reads) parts.push(`lu ${plural(reads, "fichier")}`);
    if (count((n) => n === "list_dir")) parts.push("parcouru le projet");
    if (count((n) => n === "grep")) parts.push("recherche code");
    const web = count((n) => n === "web_search" || n === "web_fetch");
    if (web) parts.push(web > 1 ? `${web} recherches web` : "recherche web");
    const created = paths("write_file").size;
    if (created) parts.push(`créé ${plural(created, "fichier")}`);
    const edited = paths("edit_file").size;
    if (edited) parts.push(`modifié ${plural(edited, "fichier")}`);
    const dirs = paths("make_dir").size;
    if (dirs) parts.push(`créé ${plural(dirs, "dossier")}`);
    const moved = paths("move_path").size;
    if (moved) parts.push(`déplacé ${plural(moved, "élément")}`);
    const removed = paths("delete_path").size;
    if (removed) parts.push(`supprimé ${plural(removed, "élément")}`);
    const delegated = count((n) => n === "delegate");
    if (delegated) parts.push(`délégué ${plural(delegated, "tâche")}`);
    // Appels de connecteurs : regroupés par serveur (« 38 appels Notion »), pas en une ligne par appel.
    const servers = new Map<string, number>();
    for (const t of settled) {
      if (!t.name.startsWith("mcp__")) continue;
      const server = (t.name.split("__")[1] ?? "MCP").replace(/_/g, " ");
      servers.set(server, (servers.get(server) ?? 0) + 1);
    }
    for (const [server, n] of servers) parts.push(`${plural(n, "appel")} ${server[0].toUpperCase() + server.slice(1)}`);
    const skills = count((n) => n === "use_skill");
    if (count((n) => n === "search_skills")) parts.push("recherche de skills");
    if (skills) parts.push(skills > 1 ? `${skills} skills utilisés` : "skill utilisé");
    if (!parts.length) parts.push(plural(settled.length, "action"));
    let add = 0;
    let del = 0;
    for (const t of settled) {
      for (const l of t.diff?.split("\n") ?? []) {
        if (l.startsWith("+") && !l.startsWith("+++")) add++;
        else if (l.startsWith("-") && !l.startsWith("---")) del++;
      }
    }
    const text = parts.join(", ");
    return { text: text[0].toUpperCase() + text.slice(1), add, del, errors: settled.filter((t) => t.state === "error" || t.state === "denied").length };
  });

  const images = $derived((msg.attachments ?? []).filter((a) => a.kind === "image"));
  const files = $derived((msg.attachments ?? []).filter((a) => a.kind === "text"));
  const onpreview = (code: string, lang: string) => (chat.preview = { code, lang: normLang(lang) });
</script>

{#if msg.role === "system"}
  <details class="summary">
    <summary>Conversation compactée — résumé des messages précédents</summary>
    <div class="md">{@html html || renderMarkdown(msg.content)}</div>
  </details>
{:else if msg.role === "user"}
  <div class="user">
    {#if editing}
      <div class="edit">
        <textarea bind:value={draft} rows="3" aria-label="Modifier ton message"></textarea>
        <div class="row">
          <button onclick={() => (editing = false)}>Annuler</button>
          <button class="primary" onclick={saveEdit} disabled={!draft.trim()}>Envoyer</button>
        </div>
      </div>
    {:else}
      <div class="col">
        {#if images.length || files.length}
          <div class="atts">
            {#each images as a}<img src={`data:image/*;base64,${a.data}`} alt={a.name} />{/each}
            {#each files as a}<span class="file">📄 {a.name}</span>{/each}
          </div>
        {/if}
        {#if msg.content}<div class="bubble">{msg.content}</div>{/if}
        {#if !chat.busy}
          <div class="actions"><button onclick={startEdit} title="Modifier le message">Modifier</button><button onclick={() => chat.branchFrom(index)} title="Repartir d'ici dans une nouvelle conversation : l'original est conservé">Branche</button></div>
        {/if}
      </div>
    {/if}
  </div>
{:else}
  <div class="assistant">
    <span class="av" aria-hidden="true"><Logo size={26} /></span>
    <div class="body">
    {#if msg.thinking}
      <details><summary>Réflexion</summary><pre>{msg.thinking}</pre></details>
    {/if}
    {#if msg.tasks?.length}
      <div class="tasks" aria-label="Liste de tâches">
        {#each msg.tasks as t}
          <div class="task {t.status}">
            <span class="box" aria-hidden="true">{t.status === "completed" ? "✓" : t.status === "in_progress" ? "…" : ""}</span>
            <span>{t.content}</span>
          </div>
        {/each}
        <div class="count">{msg.tasks.filter((t) => t.status === "completed").length} / {msg.tasks.length}</div>
      </div>
    {/if}
    {#if settled.length}
      <button class="fold" onclick={() => (showAll = !showAll)} aria-expanded={showAll}>
        <span class="chev" class:open={showAll}><Icon name="chevron" size={14} /></span>
        <span class="txt">{stats.text}</span>
        {#if stats.errors}<span class="errs">{plural(stats.errors, "erreur")}</span>{/if}
        {#if stats.add || stats.del}
          <span class="diffstat">{#if stats.add}<b class="plus">+{stats.add}</b>{/if}{#if stats.del}<b class="minus">−{stats.del}</b>{/if}</span>
        {/if}
      </button>
    {/if}
    {#each timeline as item, i (("tool" in item ? "t:" + item.tool.call_id : "note:" + i))}
      {#if "tool" in item}
        <ToolCard tool={item.tool} {chat} />
      {:else}
        <div class="note">{@html renderMarkdown(item.note)}</div>
      {/if}
    {/each}
    {#if undoable}
      <div class="changes">
        <span>{written.length} fichier{written.length > 1 ? "s" : ""} modifié{written.length > 1 ? "s" : ""} :
          <code>{written.join(", ")}</code></span>
        <button onclick={() => chat.undoMessage(index)}>Tout annuler</button>
      </div>
    {/if}
    {#if streaming && !answer && !msg.tools?.length}
      <span class="typing"><i></i><i></i><i></i></span>
    {:else if answer}
      <div class="md" use:enhance={{ html, final: !streaming, onpreview }}>{@html html}</div>
      {#if images.length}
        <div class="gen">
          {#each images as a}
            <figure>
              <img src={`data:image/*;base64,${a.data}`} alt={a.name} />
              <figcaption>{a.name}{#if a.path} · <button class="linkbtn" onclick={() => invoke("reveal_file", { path: a.path }).catch(() => {})}>Afficher dans le dossier</button>{/if}</figcaption>
            </figure>
          {/each}
        </div>
      {/if}
    {/if}
    {#if last && !streaming && chat.planReady}
      <div class="plan" role="group" aria-label="Plan prêt">
        <span>Plan prêt : discute-le, ou lance l'exécution.</span>
        <button class="primary" onclick={() => chat.approvePlan()}>Approuver et exécuter</button>
      </div>
    {/if}
    {#if !streaming && answer}
      <div class="actions">
        <button onclick={copy}>{copied ? "Copié" : "Copier"}</button>
        <button onclick={() => chat.exportText(chat.convs.find((c) => c.id === chat.activeId)?.title || "Réponse", answer)} title="Exporter en PDF, Word, Excel, CSV (les tableaux), page web ou Markdown">Exporter</button>
        {#if canSpeak}<button onclick={toggleSpeak}>{speaking ? "Arrêter la lecture" : "Lire"}</button>{/if}
        {#if last}<button onclick={() => chat.regenerate()}>Régénérer</button>{/if}
      </div>
    {/if}
    </div>
  </div>
{/if}

<style>
  .gen { display: flex; flex-wrap: wrap; gap: 10px; margin: 8px 0; }
  .gen figure { margin: 0; max-width: min(100%, 420px); }
  .gen img { display: block; width: 100%; border-radius: 12px; border: 1px solid var(--border); }
  .gen figcaption { color: var(--muted); font-size: 12px; margin-top: 4px; }
  .linkbtn { border: 0; background: none; padding: 0; color: var(--accent); text-decoration: underline; font-size: inherit; }
  .summary { margin: 16px 0; border: 1px dashed var(--border); border-radius: 12px; padding: 8px 14px; color: var(--muted); font-size: 14px; }
  .summary summary { cursor: pointer; }
  .summary .md { margin-top: 8px; font-size: 14px; }
  .user { display: flex; justify-content: flex-end; margin: 22px 0 10px; animation: msgin 0.35s cubic-bezier(0.2, 0.7, 0.2, 1) both; }
  .col { display: flex; flex-direction: column; align-items: flex-end; gap: 6px; max-width: 85%; }
  .bubble { background: var(--user); border-radius: 18px; padding: 10px 16px; white-space: pre-wrap; line-height: 1.55; overflow-wrap: anywhere; }
  .atts { display: flex; flex-wrap: wrap; gap: 6px; justify-content: flex-end; }
  .atts img { max-height: 140px; max-width: 220px; border-radius: 10px; border: 1px solid var(--border); }
  .file { background: var(--user); border-radius: 8px; padding: 4px 10px; font-size: 13px; }
  .edit { width: 85%; display: flex; flex-direction: column; gap: 8px; }
  .edit textarea { border: 1px solid var(--accent); background: var(--surface); border-radius: 14px; padding: 10px 14px; outline: none; resize: vertical; line-height: 1.5; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  .row button { border-radius: 8px; padding: 5px 14px; border: 1px solid var(--border); background: none; }
  .row .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .actions { display: flex; gap: 4px; opacity: 0; transition: opacity 0.15s; }
  .user:hover .actions, .assistant:hover .actions { opacity: 1; }
  .actions button { border: 0; background: none; color: var(--muted); font-size: 13px; padding: 3px 8px; border-radius: 6px; }
  .actions button:hover { background: var(--hover); color: var(--fg); }
  .assistant { display: grid; grid-template-columns: 26px minmax(0, 1fr); gap: 14px; margin: 10px 0 24px; animation: msgin 0.35s cubic-bezier(0.2, 0.7, 0.2, 1) both; }
  .assistant .av { margin-top: 2px; }
  .assistant .body { min-width: 0; }
  .fold { display: flex; align-items: center; gap: 8px; width: 100%; border: 0; background: none; border-radius: 8px; padding: 6px 4px; margin: 6px 0; color: var(--muted); font-family: var(--sans); font-size: 14px; text-align: left; }
  .fold:hover { color: var(--fg); background: var(--hover); }
  .fold .txt { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chev { display: inline-flex; flex: none; transform: rotate(-90deg); transition: transform 0.15s; }
  .chev.open { transform: none; }
  .note { margin: 4px 0 4px 6px; padding-left: 12px; border-left: 2px solid var(--border); color: var(--muted); font-size: 14.5px; line-height: 1.5; }
  .note :global(p) { margin: 0.25em 0; }
  .errs { color: var(--del); font-size: 13px; white-space: nowrap; }
  .diffstat { display: inline-flex; gap: 4px; flex: none; }
  .diffstat b { font-weight: 600; font-size: 12px; padding: 1px 6px; border-radius: 5px; }
  .plus { background: var(--add-bg); color: var(--add); }
  .minus { background: var(--del-bg); color: var(--del); }
  .tasks { border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 8px 12px; margin: 8px 0; font-family: var(--sans); font-size: 14px; display: flex; flex-direction: column; gap: 5px; }
  .task { display: flex; align-items: flex-start; gap: 9px; }
  .box { flex: none; width: 17px; height: 17px; margin-top: 1px; border: 1.5px solid var(--border); border-radius: 5px; display: grid; place-items: center; font-size: 12px; line-height: 1; }
  .task.completed { color: var(--muted); }
  .task.completed .box { background: var(--add); border-color: var(--add); color: white; }
  .task.in_progress { font-weight: 600; }
  .task.in_progress .box { border-color: var(--accent); color: var(--accent); }
  .count { align-self: flex-end; font-size: 12px; color: var(--muted); }
  .plan { display: flex; align-items: center; gap: 12px; border: 1px solid var(--accent); background: var(--surface); border-radius: 12px; padding: 10px 14px; margin: 12px 0 4px; font-family: var(--sans); font-size: 14px; color: var(--muted); }
  .plan span { flex: 1; }
  .plan .primary { border: 1px solid var(--accent); background: var(--accent); color: var(--accent-fg); border-radius: 9px; padding: 6px 14px; font-weight: 500; }
  .plan .primary:hover { filter: brightness(1.08); }
  .changes { display: flex; align-items: center; gap: 12px; border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 8px 12px; margin: 8px 0; font-family: var(--sans); font-size: 13px; color: var(--muted); }
  .changes span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .changes code { color: var(--fg); }
  .changes button { border: 1px solid var(--border); background: none; border-radius: 8px; padding: 4px 12px; color: var(--fg); }
  .changes button:hover { border-color: var(--accent); color: var(--accent); }
  details { color: var(--muted); margin-bottom: 8px; font-size: 14px; }
  pre { white-space: pre-wrap; font-family: inherit; }
  .typing { display: inline-flex; gap: 4px; padding: 8px 0; }
  .typing i { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); animation: bounce 1.1s infinite ease-in-out; }
  .typing i:nth-child(2) { animation-delay: 0.15s; }
  .typing i:nth-child(3) { animation-delay: 0.3s; }
  @keyframes pulse { 0%, 80%, 100% { opacity: 0.25; } 40% { opacity: 1; } }
</style>
