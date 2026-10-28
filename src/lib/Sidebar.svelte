<script lang="ts">
  import type { ChatStore, Conv } from "$lib/chat.svelte";
    import Icon from "$lib/Icon.svelte";
  import { initialOf, type Profile } from "$lib/profiles";
  import Select from "$lib/Select.svelte";
  import { stepOf, titleOf } from "$lib/progress";

  let { chat, profile, onswitch, onmodels, onsettings }: { chat: ChatStore; profile: Profile; onswitch: () => void; onmodels: () => void; onsettings: () => void } = $props();
  /** Connecté = clé acceptée par Ollama Cloud (ou catalogue chargé, clé pas encore testée). */
  const connected = $derived(chat.cloudStatus === "ok" || chat.cloudStatus === "listed");
  let renaming = $state<string | null>(null);
  let draft = $state("");

  function startRename(c: { id: string; title: string }) {
    renaming = c.id;
    draft = c.title;
  }

  const pinned = $derived(chat.convs.filter((c) => c.pinned));
  const recent = $derived(chat.convs.filter((c) => !c.pinned));
  let openPinned = $state(true);
  let openRecent = $state(true);
  function toggle(which: "pinned" | "recent") {
    if (which === "pinned") openPinned = !openPinned;
    else openRecent = !openRecent;
  }

  async function finishRename(c: Conv) {
    if (renaming !== c.id) return;
    renaming = null;
    await chat.renameConv(c, draft);
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<aside>
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={chat.mode === "chat"} class:on={chat.mode === "chat"} onclick={() => chat.setMode("chat")}>Chat</button>
    <button role="tab" aria-selected={chat.mode === "code"} class:on={chat.mode === "code"} onclick={() => chat.setMode("code")}>Code</button>
  </div>

  <input class="search" placeholder="Rechercher…" aria-label="Rechercher dans les conversations" bind:value={chat.search} oninput={() => chat.loadConvs()} />

  <nav>
    <button class="navb" onclick={() => chat.newChat()}><span class="plus">+</span> {chat.mode === "chat" ? "Nouveau" : "Nouvelle session"}</button>
    <button class="navb" class:on={chat.view === "projects" || chat.view === "project"} onclick={() => chat.showProjects()}><Icon name="folder" size={17} /> Projets</button>
    <button class="navb" class:on={chat.view === "tasks"} onclick={() => chat.showTasks()}><Icon name="clock" size={17} /> Tâches planifiées</button>
    {#if chat.mode === "chat"}
      <button class="navb" class:on={chat.view === "artifacts"} onclick={() => chat.showArtifacts()}><Icon name="book" size={17} /> Artéfacts</button>
    {/if}
  </nav>

  {#snippet row(c: Conv)}
    <li class:active={c.id === chat.activeId && chat.view === "chat"}>
      {#if renaming === c.id}
        <input
          class="rename"
          aria-label="Nouveau nom de la conversation"
          use:focusSelect
          bind:value={draft}
          onblur={() => finishRename(c)}
          onkeydown={(e) => { if (e.key === "Enter") finishRename(c); if (e.key === "Escape") renaming = null; }}
        />
      {:else}
        <button class="title" onclick={() => chat.open(c)} ondblclick={() => startRename(c)} title={(c.workspace ?? c.title) + " — double-clic pour renommer"}>
          {#if chat.isRunning(c.id)}<span class="run" role="img" aria-label="En cours" title="Cette conversation travaille en ce moment"></span>{/if}{c.title}
          {#if c.snippet}<small class="snip">{c.snippet}</small>{:else if c.workspace}<small>{c.workspace.split(/[\\/]/).filter(Boolean).pop()}</small>{/if}
        </button>
      {/if}
      {#if chat.modeProjects.length}
        <span class="move">
          <Select
            title="Déplacer vers un projet"
            value={c.project_id ?? ""}
            items={[{ value: "", label: "Sans projet" }, ...chat.modeProjects.map((p) => ({ value: p.id, label: p.name }))]}
            onchange={(v) => chat.moveToProject(c, v || null)}
          >
            {#snippet trigger()}<span class="mv"><Icon name="folder" size={14} /></span>{/snippet}
          </Select>
        </span>
      {/if}
      <button class="del" class:on={c.pinned} title={c.pinned ? "Désépingler" : "Épingler en haut de la liste"} aria-label={c.pinned ? `Désépingler ${c.title}` : `Épingler ${c.title}`} aria-pressed={!!c.pinned} onclick={() => chat.pin(c)}><Icon name="pin" size={14} /></button>
      <button class="del" title="Exporter (PDF, Word, Excel, CSV, page web, Markdown, JSON)" aria-label="Exporter la conversation {c.title}" onclick={() => chat.exportConv(c)}><Icon name="download" size={14} /></button>
      <button class="del" title="Supprimer" aria-label="Supprimer la conversation {c.title}" onclick={() => chat.remove(c)}><Icon name="close" size={14} /></button>
    </li>
  {/snippet}

  <div class="lists">
  {#if pinned.length}
    <button class="sect" class:closed={!openPinned} aria-expanded={openPinned} onclick={() => toggle("pinned")}>Épinglés <Icon name="chevron" size={14} /></button>
    {#if openPinned}<ul class="pins">{#each pinned as c (c.id)}{@render row(c)}{/each}</ul>{/if}
  {/if}

  <div class="sectrow">
    <button class="sect" class:closed={!openRecent} aria-expanded={openRecent} onclick={() => toggle("recent")}>
      {chat.currentProject && chat.activeProject ? `Récents · ${chat.currentProject.name}` : "Récents"} <Icon name="chevron" size={14} />
    </button>
    {#if chat.modeProjects.length}
      <Select
        title="Filtrer par projet"
        value={chat.activeProject ?? ""}
        items={[{ value: "", label: "Tous les projets" }, ...chat.modeProjects.map((p) => ({ value: p.id, label: p.name }))]}
        onchange={(v) => chat.filterProject(v || null)}
      >
        {#snippet trigger()}<span class="flt" class:on={!!chat.activeProject}><Icon name="settings" size={16} /></span>{/snippet}
      </Select>
    {/if}
  </div>
  {#if openRecent}
    <ul class="list">
      {#each recent as c (c.id)}{@render row(c)}{:else}<li class="none">{pinned.length ? "Rien d'autre" : "Aucune conversation"}</li>{/each}
    </ul>
  {/if}
  </div>

  {#if chat.gamify}
    {@const st = stepOf(chat.progress.xp)}
    <button class="lvl" onclick={() => chat.openProgress()} title="Ta progression : {st.into} / {st.need} points pour le niveau {st.level + 1}">
      <span class="lvtop">
        <span class="n">Niv. {st.level}</span>
        <span class="ttl">{titleOf(st.level)}</span>
        {#if chat.progress.streak >= 2}<span class="flame" title="{chat.progress.streak} jours de suite"><Icon name="flame" size={13} />{chat.progress.streak}</span>{/if}
        {#key chat.xpGain.at}{#if chat.xpGain.n}<span class="gain">+{chat.xpGain.n}</span>{/if}{/key}
      </span>
      <span class="xpbar"><span style:width="{st.ratio * 100}%"></span></span>
    </button>
  {/if}

  <div class="foot">
    <button class="profile" onclick={onswitch} disabled={chat.anyRunning} title={chat.anyRunning ? "Arrête les générations en cours avant de changer de profil" : "Changer de profil"}>
      <span class="avatar" style:background={profile.color}>{initialOf(profile.name)}</span>
      <span class="who">
        <span class="pname">{profile.name}</span>
        <span class="st" title={chat.cloudStatus === "error" ? chat.cloudError : ""}><span class="dot" class:on={connected}></span>{connected ? "Connecté" : "Non connecté"}</span>
      </span>
    </button>
    <button class="ico" onclick={onmodels} title="Modèles (Ctrl+M)" aria-label="Modèles"><Icon name="download" size={17} /></button>
    <button class="ico" onclick={onsettings} title="Réglages (Ctrl+,)" aria-label="Réglages"><Icon name="settings" size={17} /></button>
  </div>
</aside>

<style>
  aside { width: 272px; flex-shrink: 0; background: var(--sidebar); border-right: 1px solid var(--border); display: flex; flex-direction: column; gap: 10px; padding: 14px 12px; }
  nav { display: grid; gap: 1px; }
  .navb { display: flex; align-items: center; gap: 10px; border: 0; background: none; color: var(--fg); font: inherit; font-size: 14.5px; padding: 7px 10px; border-radius: 9px; text-align: left; }
  .navb:hover, .navb.on { background: var(--hover); }
  .sect { display: flex; align-items: center; gap: 4px; border: 0; background: none; color: var(--muted); font: inherit; font-size: 13.5px; padding: 6px 10px; text-align: left; border-radius: 8px; }
  .sect:hover { color: var(--fg); }
  .sect.closed :global(svg) { transform: rotate(-90deg); }
  .sectrow { display: flex; align-items: center; justify-content: space-between; padding-right: 6px; }
  .flt { display: grid; place-items: center; width: 26px; height: 26px; border-radius: 6px; color: var(--muted); }
  .flt:hover, .flt.on { background: var(--hover); color: var(--accent); }
  .run { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--accent); margin-right: 6px; animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .tabs { display: grid; grid-template-columns: 1fr 1fr; background: var(--hover); border-radius: 10px; padding: 3px; }
  .tabs button { border: 0; background: none; padding: 6px; border-radius: 8px; color: var(--muted); }
  .tabs button.on { background: var(--surface); color: var(--fg); box-shadow: 0 1px 2px rgb(0 0 0 / 0.08); }
  .plus { display: grid; place-items: center; width: 20px; height: 20px; border-radius: 50%; background: var(--fg); color: var(--bg); line-height: 1; font-size: 15px; }
  .search { border: 1px solid var(--border); background: var(--bg); border-radius: 10px; padding: 7px 10px; outline: none; }
  .search:focus { border-color: var(--accent); }
  .move { opacity: 0; color: var(--muted); display: inline-flex; }
  li:hover .move, .move:focus-within { opacity: 1; }
  .mv { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 6px; }
  .mv:hover { background: var(--surface); color: var(--fg); }
  ul { list-style: none; margin: 0; padding: 0; }
  .lists { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; }
  li { display: flex; align-items: center; border-radius: 10px; transition: background-color 0.15s; animation: fadein 0.3s ease-out both; }
  li:hover, li.active { background: var(--hover); }
  li.none { padding: 8px 10px; color: var(--muted); font-size: 13px; }
  .title { flex: 1; min-width: 0; text-align: left; border: 0; background: none; padding: 8px 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title small { display: block; color: var(--muted); font-size: 12px; }
  .del { border: 0; background: none; color: var(--muted); padding: 4px 6px; opacity: 0; }
  li:hover .del { opacity: 1; }
  .del.on { opacity: 1; color: var(--accent); }
  .snip { display: block; white-space: normal; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; color: var(--muted); font-size: 12px; margin-top: 2px; }
  .profile:hover:not(:disabled) { background: var(--hover); }
  .profile:disabled { opacity: 0.55; }
  .avatar { display: grid; place-items: center; width: 26px; height: 26px; border-radius: 7px; color: #fff; font-weight: 600; font-size: 14px; flex: none; }
  .ico { display: grid; place-items: center; border: 0; background: none; color: var(--muted); width: 28px; height: 28px; border-radius: 8px; }
  .ico:hover { background: var(--hover); color: var(--fg); }
  .rename { flex: 1; min-width: 0; margin: 3px 6px; border: 1px solid var(--accent); background: var(--bg); border-radius: 8px; padding: 5px 8px; outline: none; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--del); }
  .dot.on { background: var(--add); }
  .lvl { position: relative; display: grid; gap: 6px; border: 1px solid var(--border); background: var(--bg); border-radius: 10px; padding: 8px 10px; text-align: left; }
  .lvl:hover { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); }
  .lvtop { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
  .lvtop .n { font-weight: 600; }
  .lvtop .ttl { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0; }
  .flame { display: inline-flex; align-items: center; gap: 2px; color: var(--accent); font-weight: 600; }
  .gain { position: absolute; right: 10px; top: 4px; color: var(--accent); font-weight: 700; font-size: 13px; pointer-events: none; animation: floatup 1.4s ease-out forwards; }
  @keyframes floatup { from { opacity: 0; transform: translateY(6px); } 20% { opacity: 1; } to { opacity: 0; transform: translateY(-16px); } }
  .xpbar { height: 5px; border-radius: 3px; background: var(--hover); overflow: hidden; }
  .xpbar span { display: block; height: 100%; background: linear-gradient(90deg, var(--accent), #e8a84a); border-radius: 3px; transition: width 0.8s cubic-bezier(0.2, 0.8, 0.2, 1); }
  .foot { display: flex; align-items: center; gap: 2px; border-top: 1px solid var(--border); padding-top: 10px; }
  .profile { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; border: 0; background: none; border-radius: 10px; padding: 5px 8px; text-align: left; }
  .who { display: grid; min-width: 0; line-height: 1.25; }
  .pname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 14px; }
  .st { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; }
</style>
