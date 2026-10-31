<script lang="ts">
  import { BUILTIN_COMMANDS, CAP_LABELS, COMPACT_AT, type ChatStore } from "$lib/chat.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "$lib/Icon.svelte";
  import Select from "$lib/Select.svelte";
  import Menu from "$lib/Menu.svelte";
  import { EFFORTS, PERMISSION_MODES } from "$lib/code-options";
  import { CREATIONS } from "$lib/artifacts";
  import { HEAVY_TOKENS } from "$lib/attachments";

  let { chat, placeholder = "" }: { chat: ChatStore; placeholder?: string } = $props();
  let text = $state("");
  let box = $state<HTMLTextAreaElement>();

  // Une suggestion remplit la zone de saisie sans envoyer : on peut la compléter avant de valider.
  $effect(() => {
    if (!chat.draft) return;
    text = chat.draft;
    chat.draft = "";
    queueMicrotask(() => {
      box?.focus();
      box?.setSelectionRange(text.length, text.length);
    });
  });
  let pick = $state(0);

  async function submit() {
    const t = text;
    if (chat.busy) {
      // Tâche en cours : la précision est ajoutée sans l'interrompre.
      if (!t.trim() || t.trim().startsWith("/")) return;
      text = "";
      await chat.enqueue(t);
      return;
    }
    if ((!t.trim() && !chat.pending.length) || chat.compacting || !chat.canSend) return;
    text = "";
    await chat.send(t);
  }

  /** Suggestions de commandes slash pendant la saisie du premier mot. */
  const suggestions = $derived.by(() => {
    const m = /^\/([\w-]*)$/.exec(text);
    if (!m) return [];
    const q = m[1].toLowerCase();
    return [
      ...BUILTIN_COMMANDS.map((c) => ({ name: c.name, hint: c.hint, skill: false })),
      ...chat.skills
        .filter((s) => s.enabled && s.user_invocable !== false)
        .map((s) => ({ name: s.name, hint: (s.argument_hint ? s.argument_hint + " — " : "") + (s.description || "Skill"), skill: true })),
    ].filter((c) => c.name.toLowerCase().startsWith(q));
  });

  function complete(name: string) {
    text = `/${name} `;
    pick = 0;
  }

  /** Dictée vocale de Windows (Win + H) : le texte dicté arrive dans la zone de saisie. */
  async function dictate() {
    box?.focus();
    try {
      await invoke("voice_dictation");
    } catch (e) {
      chat.error = `Dictée impossible : ${e}`;
    }
  }

  /** Ctrl+V : une image du presse-papiers (capture d'écran…) est jointe, mais seulement si le modèle gère les images. */
  async function onPaste(e: ClipboardEvent) {
    const images = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith("image/"));
    if (!images.length || e.clipboardData?.getData("text/plain").trim()) return;
    e.preventDefault();
    for (const f of images) await chat.addPastedImage(f);
  }

  function onKey(e: KeyboardEvent) {
    if (suggestions.length) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        pick = (pick + (e.key === "ArrowDown" ? 1 : -1) + suggestions.length) % suggestions.length;
        return;
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey && suggestions[pick].name !== text.slice(1))) {
        e.preventDefault();
        complete(suggestions[Math.min(pick, suggestions.length - 1)].name);
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  }

  /** Ajuste la hauteur au contenu ; recalculée quand le texte ou la largeur changent. */
  function grow(node: HTMLTextAreaElement, _value: string) {
    const fit = () => {
      node.style.height = "auto";
      node.style.height = Math.min(node.scrollHeight, 220) + "px";
    };
    // La mise en page n'existe pas encore au montage : on mesure à l'image suivante.
    requestAnimationFrame(fit);
    const ro = new ResizeObserver(() => requestAnimationFrame(fit));
    if (node.parentElement) ro.observe(node.parentElement);
    return { update: () => requestAnimationFrame(fit), destroy: () => ro.disconnect() };
  }

  const ratio = $derived(chat.ctxLimit > 0 ? Math.min(1, chat.promptTokens / chat.ctxLimit) : 0);
  const fmt = (n: number) => (n >= 1000 ? (n / 1000).toFixed(1).replace(".0", "") + "k" : String(n));
</script>

<div class="wrap">
  {#if suggestions.length}
    <ul class="slash" role="listbox">
      {#each suggestions as s, i}
        <li>
          <button class:on={i === pick} role="option" aria-selected={i === pick} onmousedown={(e) => { e.preventDefault(); complete(s.name); }}>
            <strong>/{s.name}</strong>{#if s.skill}<span class="tag">skill</span>{/if}
            <span class="hint">{s.hint}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if chat.notice}<div class="notice">{chat.notice}</div>{/if}

  <div class="composer">
    {#if chat.pending.length}
      <div class="chips">
        {#each chat.pending as a, i}
          <span class="chip">
            {#if a.kind === "image"}<img src={`data:image/*;base64,${a.data}`} alt="" />{:else}📄{/if}
            {a.name}
            {#if a.kind === "text" && a.tokens}
              <span class="tok" title="Poids estimé en jetons">≈ {fmt(a.tokens)}</span>
              {#if a.mode && a.tokens > HEAVY_TOKENS && chat.supportsTools}
                <button class="mode" onclick={() => (a.mode = a.mode === "excerpt" ? "full" : "excerpt")}
                  title={a.mode === "excerpt" ? "Seuls des extraits sont envoyés ; le modèle lit la suite à la demande. Cliquer pour tout envoyer." : "Le fichier entier est envoyé à chaque message. Cliquer pour n'envoyer que des extraits."}>{a.mode === "excerpt" ? "Extraits" : "Entier"}</button>
              {/if}
            {/if}
            <button aria-label="Retirer {a.name}" onclick={() => chat.pending.splice(i, 1)}>×</button>
          </span>
        {/each}
      </div>
    {/if}
    {#if chat.creating}
      <div class="making"><span>Création : {CREATIONS[chat.creating].label}</span><button onclick={() => (chat.creating = null)} aria-label="Annuler la création" title="Revenir à un chat normal">×</button></div>
    {/if}
    {#if chat.queued.length}
      <div class="queued" role="status">
        {#each chat.queued as q}<span title={q}>↳ {q.length > 70 ? q.slice(0, 70) + "…" : q}<em>pris en compte à la prochaine étape</em></span>{/each}
      </div>
    {/if}
    <textarea
      use:grow={text}
      bind:this={box}
      bind:value={text}
      oninput={() => (pick = 0)}
      onkeydown={onKey}
      onpaste={onPaste}
      rows="1"
      aria-label="Ton message"
      placeholder={chat.busy ? "Ajoute une précision : elle sera prise en compte sans interrompre la tâche…" : placeholder || (chat.mode === "chat" ?"Écris un message ou / pour les commandes" : "Décris la tâche ou / pour les commandes")}
    ></textarea>
    <div class="row">
      <button class="attach" title={chat.supportsVision || !chat.caps[chat.model] ? "Joindre des fichiers ou images" : "Joindre des fichiers (ce modèle ne lit pas les images)"} aria-label="Joindre" onclick={() => chat.pickFiles()}><Icon name="plus" size={18} /></button>
      <button class="tool" title="Dicter (dictée vocale de Windows, Win + H)" aria-label="Dicter" onclick={dictate}><Icon name="mic" size={17} /></button>
      <button
        class="tool"
        class:on={chat.researchNext}
        title={chat.researchNext ? "Recherche approfondie activée pour le prochain message" : "Recherche approfondie : plusieurs agents explorent le web en parallèle, puis rédigent un rapport avec sources (plus long, plus de jetons)"}
        aria-pressed={chat.researchNext}
        aria-label="Recherche approfondie"
        onclick={() => (chat.researchNext = !chat.researchNext)}
      ><Icon name="search" size={17} /></button>
      <button
        class="tool"
        class:on={chat.online}
        title={chat.online ? "Recherche web active (DuckDuckGo). Elle se désactive toute seule si l'ordinateur est hors ligne." : "Hors ligne : la recherche web est indisponible. Clique pour revérifier la connexion."}
        aria-label={chat.online ? "Recherche web active" : "Hors ligne"}
        onclick={() => chat.checkOnline(true)}
      ><Icon name="globe" size={17} /></button>
      <Menu title="Mode" icon="shield" items={PERMISSION_MODES} value={chat.permission} onselect={(id) => chat.setPermission(id)} />
      {#if chat.mode === "code"}
        <Menu
          title="Effort"
          icon="gauge"
          items={EFFORTS}
          value={chat.effort}
          onselect={(id) => chat.setEffort(id)}
          hint="Un effort plus élevé donne des réponses plus approfondies, mais prend plus de temps et utilise plus de mémoire."
        />
      {/if}
      {#if chat.caps[chat.model]?.includes("thinking")}
        <Select
          up
          flat
          value={chat.think}
          items={[{ value: "auto", label: "Réflexion auto" }, { value: "on", label: "Réflexion activée" }, { value: "off", label: "Réflexion désactivée" }]}
          title="Réflexion du modèle (raisonnement avant de répondre)"
          disabled={chat.busy}
          onchange={(v) => chat.setThink(v as "auto" | "on" | "off")}
        />
      {/if}
      <Select
        up
        flat
        value={chat.model}
        items={chat.chatModels.map((m) => ({ value: m.name, label: m.label ?? m.name }))}
        placeholder="Aucun modèle"
        title="Modèle"
        disabled={!chat.models.length || chat.busy}
        onchange={(v) => chat.setModel(v)}
      />
      {#if chat.capsOf(chat.model).length}
        <span class="caps" title="Capacités annoncées par Ollama pour ce modèle">
          {#each chat.capsOf(chat.model) as c}<span class="cap">{CAP_LABELS[c]}</span>{/each}
        </span>
      {/if}
      <span class="spacer"></span>
      {#if chat.promptTokens > 0 || chat.compacting}
        <button
          class="ctx"
          class:warn={ratio > 0.8}
          title="Contexte utilisé : {chat.promptTokens} / {chat.ctxLimit} jetons. Compactage automatique à {Math.round(COMPACT_AT * 100)} %. Cliquer pour compacter maintenant."
          disabled={chat.busy || chat.compacting}
          onclick={() => chat.compact()}
        >
          <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
            <circle cx="10" cy="10" r="8" fill="none" stroke="var(--border)" stroke-width="3" />
            <circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"
              stroke-dasharray="{ratio * 50.3} 50.3" transform="rotate(-90 10 10)" />
          </svg>
          {chat.compacting ? "Compactage…" : `${fmt(chat.promptTokens)} / ${fmt(chat.ctxLimit)}`}
        </button>
      {/if}
      {#if chat.busy}
        {#if text.trim()}
          <button class="send" onclick={submit} title="Ajouter à la tâche en cours (Entrée)" aria-label="Ajouter à la tâche en cours">↑</button>
        {/if}
        <button class="send stop" onclick={() => chat.stop()} title="Arrêter" aria-label="Arrêter">■</button>
      {:else}
        <button class="send" onclick={submit} disabled={(!text.trim() && !chat.pending.length) || !chat.canSend || chat.compacting} title="Envoyer" aria-label="Envoyer">↑</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .wrap { position: relative; }
  .caps { display: inline-flex; gap: 4px; min-width: 0; overflow: hidden; }
  .cap { font-size: 12px; color: var(--muted); background: var(--hover); border-radius: 6px; padding: 1px 7px; white-space: nowrap; }
  .composer { background: var(--surface); border: 1px solid var(--border); border-radius: 20px; padding: 12px 14px 10px; box-shadow: 0 2px 10px rgb(0 0 0 / 0.05); }
  .composer:focus-within { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); }
  textarea { width: 100%; border: 0; outline: none; resize: none; background: none; max-height: 220px; line-height: 1.5; font-size: 16px; }
  .row { display: flex; align-items: center; flex-wrap: wrap; gap: 6px 8px; margin-top: 6px; }
  .spacer { flex: 1; }
  .attach, .tool { display: grid; place-items: center; border: 0; background: none; color: var(--muted); font-size: 20px; line-height: 1; width: 30px; height: 30px; border-radius: 8px; }
  .tool.on { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
  .attach:hover, .tool:hover { background: var(--hover); color: var(--fg); }
  .making { display: inline-flex; align-items: center; gap: 6px; background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); border-radius: 8px; padding: 2px 4px 2px 10px; font-size: 13px; margin-bottom: 8px; }
  .making button { border: 0; background: none; color: inherit; font-size: 16px; line-height: 1; padding: 0 6px; }
  .queued { display: flex; flex-direction: column; gap: 4px; margin-bottom: 8px; }
  .queued span { font-size: 13px; background: var(--hover); border-radius: 8px; padding: 4px 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .queued em { color: var(--muted); font-style: normal; margin-left: 8px; font-size: 12px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .chip { display: inline-flex; align-items: center; gap: 6px; background: var(--hover); border-radius: 8px; padding: 3px 4px 3px 8px; font-size: 13px; }
  .chip img { height: 22px; width: 22px; object-fit: cover; border-radius: 4px; }
  .tok { color: var(--muted); font-size: 12px; }
  .chip .mode { border: 1px solid var(--border); border-radius: 6px; padding: 0 7px; font-size: 12px; color: var(--fg); background: var(--surface); }
  .chip button { border: 0; background: none; color: var(--muted); padding: 0 6px; font-size: 16px; }
  .ctx { display: flex; align-items: center; gap: 5px; border: 0; background: none; color: var(--muted); font-size: 12px; padding: 4px 8px; border-radius: 8px; }
  .ctx:hover:not(:disabled) { background: var(--hover); }
  .ctx.warn { color: var(--accent); }
  .send { width: 32px; height: 32px; border-radius: 9px; border: 0; background: var(--accent); color: var(--accent-fg); font-size: 17px; line-height: 1; }
  .send:disabled { opacity: 0.35; }
  .slash { position: absolute; bottom: calc(100% + 8px); left: 0; right: 0; z-index: 20; list-style: none; margin: 0; padding: 6px; background: var(--surface); border: 1px solid var(--border); border-radius: 14px; box-shadow: 0 10px 30px rgb(0 0 0 / 0.15); max-height: 260px; overflow-y: auto; }
  .slash button { display: flex; align-items: baseline; gap: 10px; width: 100%; text-align: left; border: 0; background: none; padding: 7px 10px; border-radius: 9px; }
  .slash button.on, .slash button:hover { background: var(--hover); }
  .tag { font-size: 12px; background: var(--hover); color: var(--muted); border-radius: 5px; padding: 0 6px; }
  .hint { color: var(--muted); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .notice { position: absolute; bottom: calc(100% + 8px); left: 50%; transform: translateX(-50%); background: var(--fg); color: var(--bg); border-radius: 999px; padding: 5px 14px; font-size: 13px; }
</style>
