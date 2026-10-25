<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ChatStore } from "$lib/chat.svelte";
  import FileTree from "$lib/FileTree.svelte";
  import { FsCtl } from "$lib/fs-ctl.svelte";
  import Icon from "$lib/Icon.svelte";
  import GitPanel from "$lib/GitPanel.svelte";
  import SessionPanel from "$lib/SessionPanel.svelte";
  import AppPreview from "$lib/AppPreview.svelte";
  import ReviewPanel from "$lib/ReviewPanel.svelte";

  let { chat }: { chat: ChatStore } = $props();

  let tab = $state<"session" | "files" | "git" | "review" | "preview" | "terminal">("session");
  let refreshKey = $state(0);
  let undoMsg = $state("");
  const ctl = new FsCtl(() => chat.workspace, () => refreshKey++);

  // Console : historique local des commandes lancées dans le dossier de travail.
  type Run = { cmd: string; out: string; running: boolean };
  let runs = $state<Run[]>([]);
  let cmd = $state("");
  let out = $state<HTMLElement>();

  // Rafraîchit l'arbre quand l'agent a fini de travailler.
  let wasBusy = false;
  $effect(() => {
    if (wasBusy && !chat.busy) refreshKey++;
    wasBusy = chat.busy;
  });

  async function run(e: Event) {
    e.preventDefault();
    const c = cmd.trim();
    if (!c || !chat.workspace) return;
    cmd = "";
    const entry: Run = $state({ cmd: c, out: "", running: true });
    runs.push(entry);
    try {
      entry.out = await invoke<string>("term_run", { workspace: chat.workspace, command: c });
    } catch (err) {
      entry.out = String(err);
    }
    entry.running = false;
    refreshKey++;
    queueMicrotask(() => out?.scrollTo({ top: out.scrollHeight }));
  }

  async function undo() {
    undoMsg = await chat.undo();
    refreshKey++;
    setTimeout(() => (undoMsg = ""), 4000);
  }
</script>

<aside>
  <div class="head">
    <button class:on={tab === "session"} onclick={() => (tab = "session")}>Session</button>
    <button class:on={tab === "files"} onclick={() => (tab = "files")}>Fichiers</button>
    <button class:on={tab === "git"} onclick={() => (tab = "git")}>Git</button>
    <button class:on={tab === "review"} onclick={() => (tab = "review")}>Revue{#if chat.review?.findings.length} <span class="badge">{chat.review.findings.length}</span>{/if}</button>
    <button class:on={tab === "preview"} onclick={() => (tab = "preview")}>Aperçu</button>
    <button class:on={tab === "terminal"} onclick={() => (tab = "terminal")}>Terminal</button>
    <span class="sp"></span>
    <button class="undo" onclick={undo} title="Annuler la dernière modification de l'agent"><Icon name="undo" size={14} /> Annuler</button>
  </div>
  {#if undoMsg}<p class="msg">{undoMsg}</p>{/if}

  {#if tab === "session"}
    <SessionPanel {chat} />
  {:else if tab === "git"}
    <GitPanel {chat} {refreshKey} />
  {:else if tab === "review"}
    <ReviewPanel {chat} />
  {:else if tab === "preview"}
    <AppPreview />
  {:else if !chat.workspace}
    <p class="msg">Choisis un dossier de travail.</p>
  {:else if tab === "files"}
    <div class="treebar">
      <button onclick={() => ctl.start("file", "")} title="Nouveau fichier à la racine"><Icon name="file" size={14} /> Fichier</button>
      <button onclick={() => ctl.start("dir", "")} title="Nouveau dossier à la racine"><Icon name="folder" size={14} /> Dossier</button>
      <button class="sp" onclick={() => refreshKey++} title="Actualiser" aria-label="Actualiser l'arbre"><Icon name="refresh" size={14} /></button>
    </div>
    {#if ctl.error && !ctl.draft}<p class="msg bad">{ctl.error}</p>{/if}
    <div class="tree"><FileTree workspace={chat.workspace} {ctl} {refreshKey} /></div>
  {:else}
    <div class="term" bind:this={out}>
      {#each runs as r}
        <div class="cmd">› {r.cmd}</div>
        <pre>{r.running ? "…" : r.out}</pre>
      {:else}
        <p class="msg">Les commandes s'exécutent dans le dossier du projet.</p>
      {/each}
    </div>
    <form onsubmit={run}>
      <input bind:value={cmd} placeholder="Commande PowerShell…" spellcheck="false" />
    </form>
  {/if}
</aside>

<style>
  aside { width: 340px; flex-shrink: 0; border-left: 1px solid var(--border); display: flex; flex-direction: column; background: var(--bg); min-height: 0; }
  .head { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; padding: 8px; border-bottom: 1px solid var(--border); }
  .head button { border: 0; background: none; color: var(--muted); padding: 5px 8px; border-radius: 8px; font-size: 14px; }
  .head button.on { background: var(--hover); color: var(--fg); }
  .sp { flex: 1; }
  .badge { background: var(--accent); color: var(--accent-fg); border-radius: 9px; padding: 0 6px; font-size: 11px; font-weight: 700; }
  .undo { display: inline-flex; align-items: center; gap: 5px; }
  .undo:hover { color: var(--accent); }
  .msg { color: var(--muted); font-size: 13px; padding: 10px 12px; margin: 0; }
  .treebar { display: flex; gap: 4px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .treebar button { display: inline-flex; align-items: center; gap: 5px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 3px 9px; font-size: 13px; color: var(--muted); }
  .treebar button:hover { color: var(--fg); border-color: var(--accent); }
  .treebar .sp { margin-left: auto; }
  .msg.bad { color: var(--del); }
  .tree { flex: 1; overflow: auto; padding: 6px 0; }
  .term { flex: 1; overflow: auto; padding: 8px 10px; font-family: ui-monospace, Consolas, monospace; font-size: 12.5px; }
  .cmd { color: var(--accent); margin-top: 8px; }
  pre { margin: 2px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  form { border-top: 1px solid var(--border); padding: 8px; }
  input { width: 100%; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 10px; outline: none; font-family: ui-monospace, Consolas, monospace; font-size: 13px; }
  input:focus { border-color: var(--accent); }
</style>
