<script lang="ts">
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import { explain, type ChatStore } from "$lib/chat.svelte";
  import type { Profile } from "$lib/profiles";
  import Sidebar from "$lib/Sidebar.svelte";
  import Composer from "$lib/Composer.svelte";
  import Icon from "$lib/Icon.svelte";
  import Logo from "$lib/Logo.svelte";
  import Onboarding from "$lib/Onboarding.svelte";
  import Tutorial from "$lib/Tutorial.svelte";
  import Welcome from "$lib/Welcome.svelte";
  import Message from "$lib/Message.svelte";
  import CodePanel from "$lib/CodePanel.svelte";
  import PreviewPane from "$lib/PreviewPane.svelte";
  import { collectArtifacts } from "$lib/artifacts";
  import TasksPage from "$lib/TasksPage.svelte";
  import Celebrate from "$lib/Celebrate.svelte";
  import UpdateToast from "$lib/UpdateToast.svelte";
  import AnalyticsAsk from "$lib/AnalyticsAsk.svelte";
  import { updater } from "$lib/updater.svelte";
  import ModelsDialog from "$lib/ModelsDialog.svelte";
  import TemplateDialog from "$lib/TemplateDialog.svelte";
  import ApprovalDialog from "$lib/ApprovalDialog.svelte";
  import SettingsDialog from "$lib/SettingsDialog.svelte";
  import ProjectDialog from "$lib/ProjectDialog.svelte";
  import ProjectsPage from "$lib/ProjectsPage.svelte";
  import ProjectPage from "$lib/ProjectPage.svelte";
  import ArtifactsPage from "$lib/ArtifactsPage.svelte";
  import type { DocDraft } from "$lib/chat.svelte";

  let { chat, profile, onswitch }: { chat: ChatStore; profile: Profile; onswitch: () => void } = $props();

  let showTemplates = $state(false);
  let showModels = $state(false);
  let showSettings = $state(false);
  // Une demande venue d'ailleurs (puce de niveau, félicitations) ouvre les Réglages sur la progression.
  $effect(() => {
    if (chat.settingsWanted) {
      chat.settingsWanted = false;
      showSettings = true;
    }
  });
  let dragging = $state(false);

  onMount(() => {
    const stops = [chat.init()];
    // Vérification discrète, un peu après le lancement pour ne pas ralentir le démarrage.
    const later = setTimeout(() => void updater.check(true), 8000);
    stops.push(() => clearTimeout(later));
    const drop = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") dragging = true;
      else if (p.type === "drop") {
        dragging = false;
        chat.addPaths(p.paths);
      } else dragging = false;
    });
    return () => {
      stops.forEach((s) => s());
      drop.then((off) => off());
    };
  });

  function onKey(e: KeyboardEvent) {
    if (!e.ctrlKey || e.altKey) return;
    const k = e.key.toLowerCase();
    if (k === "n") chat.newChat();
    else if (k === "k") document.querySelector<HTMLInputElement>("input.search")?.focus();
    else if (k === "m") showModels = true;
    else if (k === ",") showSettings = true;
    else if (k === "b") chat.toggleSidebar();
    else return;
    e.preventDefault();
  }

  const SUGGESTIONS = {
    chat: [
      { label: "Résumer un texte", icon: "file", text: "Résume ce texte en quelques points clés :\n\n" },
      { label: "Expliquer simplement", icon: "globe", text: "Explique-moi simplement : " },
      { label: "Rédiger un e-mail", icon: "pencil", text: "Rédige un e-mail professionnel pour " },
      { label: "Aider à déboguer", icon: "alert", text: "Aide-moi à comprendre et corriger cette erreur :\n\n" },
    ],
    code: [
      { label: "Expliquer ce projet", icon: "folder", text: "Explore le projet et explique-moi son architecture et comment le lancer." },
      { label: "Corriger les erreurs", icon: "alert", text: "Lance la vérification du projet et corrige toutes les erreurs." },
      { label: "Ajouter des tests", icon: "check", text: "Ajoute des tests pour : " },
      { label: "Relire le code", icon: "file", text: "Fais une revue de code des derniers changements et signale les problèmes." },
    ],
  };
  const suggestions = $derived(SUGGESTIONS[chat.mode]);
  const empty = $derived(chat.messages.length === 0);

  async function saveProject(name: string, instructions: string, docs: DocDraft[], removed: string[], workspace: string) {
    const d = chat.projectDialog;
    chat.projectDialog = null;
    if (d === "new") await chat.createProject(name, instructions, docs, workspace);
    else if (d) await chat.updateProject(d.id, name, instructions, docs, removed, workspace);
  }
  const folderName = $derived(chat.workspace.split(/[\\/]/).filter(Boolean).pop() ?? "");
  const noTools = $derived(chat.mode === "code" && !chat.supportsTools);
</script>

<svelte:window onkeydown={onKey} />

{#snippet errorBar(retry: boolean)}
  <div class="errbar" role="alert">
    <span class="eico"><Icon name="alert" size={18} /></span>
    <div class="etext">
      <div>{chat.error}</div>
      {#if explain(chat.error)}<div class="hintline">{explain(chat.error)}</div>{/if}
    </div>
    {#if retry}<button onclick={() => chat.regenerate()}><Icon name="refresh" size={14} /> Réessayer</button>{/if}
    <button class="x" aria-label="Fermer le message d'erreur" onclick={() => (chat.error = "")}><Icon name="close" size={14} /></button>
  </div>
{/snippet}

<div class="app">
  {#if chat.sidebarOpen}
    <Sidebar {chat} {profile} {onswitch} onmodels={() => (showModels = true)} onsettings={() => (showSettings = true)} />
  {/if}

  <section class="main">
    <button class="fold left" onclick={() => chat.toggleSidebar()} title="Barre latérale (Ctrl+B)" aria-label="Afficher ou masquer la barre latérale" aria-pressed={chat.sidebarOpen}>
      <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><rect x="2.5" y="3.5" width="15" height="13" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.5" /><path d="M8 3.5v13" stroke="currentColor" stroke-width="1.5" /></svg>
    </button>
    {#if chat.mode === "code"}
      <button class="fold right" onclick={() => chat.togglePanel()} title="Fichiers et terminal" aria-label="Afficher ou masquer le panneau fichiers et terminal" aria-pressed={chat.panelOpen}>
        <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><rect x="2.5" y="3.5" width="15" height="13" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.5" /><path d="M12 3.5v13" stroke="currentColor" stroke-width="1.5" /></svg>
      </button>
    {/if}
    {#if chat.mode === "code" && chat.view === "chat"}
      <header>
        <span class="folder-ico"><Icon name="folder" size={17} /></span>
        {#if chat.activeId}
          <strong title={chat.workspace}>{folderName}</strong>
          <span class="path">{chat.workspace}</span>
        {:else}
          <input class="path-input" placeholder="Dossier de travail" bind:value={chat.workspace} />
          <button class="browse" onclick={() => chat.pickFolder()}>Parcourir…</button>
          <button class="browse" onclick={() => (showTemplates = true)}>Modèle…</button>
        {/if}
      </header>
    {/if}

    {#if chat.unavailable[chat.model]}
      <p class="banner off" role="alert">« {chat.model} » n'est plus disponible sur Ollama Cloud (erreur 410). Choisis un autre modèle dans la liste sous la zone de saisie{chat.models.find((m) => m.name !== chat.model && !chat.unavailable[m.name]) ? ", par exemple " + chat.models.find((m) => m.name !== chat.model && !chat.unavailable[m.name])?.name : ""}}.</p>
    {/if}
    {#if noTools}
      <p class="banner">« {chat.model} » ne gère pas les outils : l'agent est désactivé, ce mode se comporte comme un chat. Choisis un modèle avec « tools » (ex. qwen3, llama3.2).</p>
    {/if}

    {#if chat.view === "projects"}
      <ProjectsPage {chat} />
    {:else if chat.view === "tasks"}
      <TasksPage {chat} />
    {:else if chat.mode === "chat" && chat.view === "artifacts"}
      <ArtifactsPage {chat} />
    {:else if chat.view === "project"}
      <ProjectPage {chat} />
    {:else if empty}
      <div class="hero">
        {#if chat.hasModels}
          <Logo size={60} animated />
          <h1 class="rise" style="--d: 0.2s">{chat.mode === "chat" ? `Bonjour ${profile.name}, que puis-je faire pour toi ?` : "Que construisons-nous ?"}</h1>
          {#if chat.mode === "code" && !chat.workspace.trim()}
            <p class="hint">Choisis d'abord un dossier de travail ci-dessus.</p>
          {/if}
          {#if chat.error}{@render errorBar(false)}{/if}
          <div class="box"><Composer {chat} /></div>
          <div class="suggestions" role="group" aria-label="Suggestions">
            {#each suggestions as s}
              <button onclick={() => (chat.draft = s.text)}><Icon name={s.icon} size={15} /> {s.label}</button>
            {/each}
          </div>
        {:else}
          <Logo size={60} />
          <h1>Bienvenue</h1>
          <div class="box"><Welcome {chat} /></div>
        {/if}
      </div>
    {:else}
      <div class="scroll" bind:this={chat.scroller}>
        <div class="thread">
          {#each chat.messages as m, i}
            <Message
              msg={m}
              index={i}
              chat={chat}
              streaming={chat.busy && i === chat.messages.length - 1}
              last={i === chat.messages.length - 1}
            />
          {/each}
          {#if chat.resumable}
            <div class="resume" role="status">
              <span>{chat.resumeKind === "cut" ? "Cette tâche a été interrompue avant la fin." : "Cette question est restée sans réponse."}</span>
              <button onclick={() => chat.resumeTask()}>{chat.resumeKind === "cut" ? "Reprendre la tâche" : "Relancer"}</button>
            </div>
          {/if}
          {#if chat.error}{@render errorBar(true)}{/if}
        </div>
      </div>
      <div class="dock"><div class="box"><Composer {chat} /></div></div>
    {/if}
  </section>

  {#if chat.pendingApprovals.length}<ApprovalDialog {chat} tool={chat.pendingApprovals[0]} more={chat.pendingApprovals.length - 1} />{/if}
  {#if showTemplates}<TemplateDialog {chat} onclose={() => (showTemplates = false)} />{/if}
  {#if chat.preview}
    <PreviewPane code={chat.preview.code} lang={chat.preview.lang} versions={collectArtifacts(chat.messages)} onselect={(a) => (chat.preview = { code: a.code, lang: a.lang })} onclose={() => (chat.preview = null)} />
  {/if}
  {#if chat.mode === "code" && chat.panelOpen}<CodePanel {chat} />{/if}
</div>

{#if dragging}<div class="drop">Dépose tes fichiers ou images ici</div>{/if}
{#if !chat.tutorialDone || chat.tutorialOpen}
  <Tutorial {chat} defaultName={profile.name} />
{:else if chat.hasModels && (!chat.onboarded || chat.onboardingOpen)}
  <Onboarding {chat} defaultName={profile.name} />
{/if}
{#if chat.projectDialog}
  <ProjectDialog project={chat.projectDialog === "new" ? null : chat.projectDialog} code={chat.projectDialog === "new" ? chat.mode === "code" : chat.projectDialog.kind === "code"} onsave={saveProject} oncancel={() => (chat.projectDialog = null)} />
{/if}
{#if showModels}<ModelsDialog {chat} onclose={() => (showModels = false)} />{/if}
<Celebrate {chat} />
{#if chat.tutorialDone && !chat.tutorialOpen && !(chat.hasModels && (!chat.onboarded || chat.onboardingOpen))}
  <AnalyticsAsk onmore={() => { chat.settingsTab = "stats"; showSettings = true; }} />
{/if}
<UpdateToast onopen={() => { chat.settingsTab = "update"; showSettings = true; }} />
{#if showSettings}<SettingsDialog {chat} profileName={profile.name} onclose={() => (showSettings = false)} />{/if}

<style>
  .app { display: flex; height: 100%; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; position: relative; }
  .fold { position: absolute; top: 9px; z-index: 5; width: 32px; height: 32px; display: grid; place-items: center; border: 0; background: none; color: var(--muted); border-radius: 8px; }
  .fold:hover { background: var(--hover); color: var(--fg); }
  .fold[aria-pressed="true"] { color: var(--fg); }
  .fold.left { left: 10px; }
  .fold.right { right: 10px; }
  header { display: flex; align-items: center; gap: 10px; padding: 10px 52px; border-bottom: 1px solid var(--border); min-height: 49px; }
  .path { color: var(--muted); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .path-input { flex: 1; border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 6px 12px; outline: none; }
  .path-input:focus { border-color: var(--accent); }
  .browse { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 6px 14px; }
  .browse:hover { border-color: var(--accent); }
  .banner.off { color: var(--del); }
  .banner { margin: 0; padding: 10px 52px; background: var(--hover); color: var(--muted); font-size: 13px; }
  .hero { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 24px; gap: 14px; }
  h1 { font-family: var(--serif); font-weight: 400; font-size: clamp(26px, 4vw, 36px); margin: 0 0 8px; text-align: center; }
  .hint { color: var(--muted); margin: 0; }
  .box { width: 100%; max-width: calc(var(--conv-w) - 40px); }
  .suggestions { display: flex; flex-wrap: wrap; justify-content: center; gap: 8px; max-width: 740px; margin-top: 6px; }
  .suggestions button { display: inline-flex; align-items: center; gap: 7px; border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 7px 14px; color: var(--muted); font-size: 14px; }
  .suggestions button:hover { color: var(--fg); border-color: var(--accent); }
  .scroll { flex: 1; overflow-y: auto; }
  .thread { max-width: var(--conv-w); margin: 0 auto; padding: 16px 28px 8px; }
  .dock { padding: 0 24px 18px; display: flex; justify-content: center; }
  .errbar { display: flex; align-items: flex-start; gap: 10px; margin: 10px 0; padding: 10px 12px; border: 1px solid color-mix(in srgb, var(--del) 45%, var(--border)); background: color-mix(in srgb, var(--del) 8%, var(--surface)); border-radius: 12px; font-size: 14px; }
  .eico { color: var(--del); display: grid; padding-top: 1px; }
  .etext { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .hintline { color: var(--muted); font-size: 13px; margin-top: 3px; }
  .errbar button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 4px 10px; display: inline-flex; align-items: center; gap: 5px; white-space: nowrap; }
  .errbar button.x { border: 0; background: none; color: var(--muted); padding: 4px; }
  .folder-ico { display: grid; color: var(--muted); }
  .resume { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin: 10px 0; padding: 10px 14px; border: 1px dashed var(--accent); border-radius: 12px; font-size: 14px; background: var(--surface); }
  .resume button { border: 1px solid var(--accent); background: var(--accent); color: var(--accent-fg); border-radius: 8px; padding: 6px 14px; }
  .drop { position: fixed; inset: 0; z-index: 60; display: grid; place-items: center; background: rgb(0 0 0 / 0.45); color: white; font-size: 22px; pointer-events: none; border: 3px dashed var(--accent); }
</style>
