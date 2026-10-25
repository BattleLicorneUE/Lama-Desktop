<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ChatStore } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";
  import Icon from "$lib/Icon.svelte";

  let { chat, refreshKey = 0 }: { chat: ChatStore; refreshKey?: number } = $props();

  type GitFile = { path: string; status: string; staged: boolean };
  type GitStatus = { repo: boolean; branch: string; files: GitFile[] };

  let st = $state<GitStatus | null>(null);
  let error = $state("");
  let selected = $state("");
  let diff = $state("");
  let message = $state("");
  let busy = $state(false);
  let branching = $state(false);
  let branchName = $state("");

  function startBranch() {
    const d = new Date();
    branchName = `lama/${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    branching = true;
  }

  async function createBranch() {
    try {
      const name = await invoke<string>("git_branch_create", { workspace: chat.workspace, name: branchName });
      chat.flash(`Branche « ${name} » créée : les prochaines validations iront dessus.`);
      branching = false;
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function load() {
    if (!chat.workspace) return;
    try {
      st = await invoke<GitStatus>("git_status", { workspace: chat.workspace });
      error = "";
      if (selected && !st.files.some((f) => f.path === selected)) {
        selected = "";
        diff = "";
      } else if (selected) await show(selected);
    } catch (e) {
      error = String(e);
    }
  }

  async function show(path: string) {
    selected = path;
    try {
      diff = await invoke<string>("git_diff", { workspace: chat.workspace, path });
    } catch (e) {
      diff = String(e);
    }
  }

  async function commit() {
    if (!st || !message.trim()) return;
    const n = st.files.length;
    const ok = await askConfirm(`Valider ${n} fichier${n > 1 ? "s" : ""} sur « ${st.branch} » avec le message « ${message.trim()} » ? Rien n'est envoyé vers un serveur.`, "Valider");
    if (!ok) return;
    busy = true;
    try {
      const out = await invoke<string>("git_commit", { workspace: chat.workspace, message });
      chat.flash(out);
      message = "";
      selected = "";
      diff = "";
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    chat.workspace;
    refreshKey;
    load();
  });

  const lines = $derived(diff ? diff.split("\n") : []);
  const LABEL: Record<string, string> = { M: "modifié", A: "ajouté", D: "supprimé", R: "renommé", "?": "non suivi", U: "conflit" };
</script>

<div class="git">
  {#if !chat.workspace}
    <p class="msg">Choisis un dossier de travail.</p>
  {:else if error}
    <p class="msg bad">{error}</p>
    <button class="small" onclick={load}>Réessayer</button>
  {:else if st && !st.repo}
    <p class="msg">Ce dossier n'est pas un dépôt git. Demande à l'agent de lancer <code>git init</code>, ou fais-le dans le terminal.</p>
  {:else if st}
    <div class="head">
      <span class="branch"><Icon name="gauge" size={14} /> {st.branch || "(sans branche)"}</span>
      <span class="acts">
        <button class="small" onclick={startBranch} title="Créer une branche de travail avant de laisser l'agent modifier le projet">+ Branche</button>
        <button class="small" onclick={load} title="Actualiser" aria-label="Actualiser l'état git"><Icon name="refresh" size={13} /></button>
      </span>
    </div>
    {#if branching}
      <form class="branchform" onsubmit={(e) => { e.preventDefault(); createBranch(); }}>
        <input bind:value={branchName} aria-label="Nom de la branche" maxlength="60" spellcheck="false" />
        <button class="small primary" disabled={!branchName.trim()}>Créer</button>
        <button type="button" class="small" onclick={() => (branching = false)}>Annuler</button>
      </form>
    {/if}

    {#if st.files.length === 0}
      <p class="msg">Aucun changement : tout est validé.</p>
    {:else}
      <ul class="files">
        {#each st.files as f (f.path)}
          <li class:on={selected === f.path}>
            <button onclick={() => show(f.path)} title="{LABEL[f.status] ?? f.status}{f.staged ? ' (indexé)' : ''}">
              <span class="chip s{f.status === '?' ? 'u' : f.status}">{f.status}</span>
              <span class="path">{f.path}</span>
            </button>
          </li>
        {/each}
      </ul>

      {#if diff}
        <pre class="diff" aria-label="Différences de {selected}">{#each lines as l}<span class:add={l.startsWith("+") && !l.startsWith("+++")} class:del={l.startsWith("-") && !l.startsWith("---")} class:hunk={l.startsWith("@@")}>{l || " "}</span>{/each}</pre>
      {/if}

      <form class="commit" onsubmit={(e) => { e.preventDefault(); commit(); }}>
        <input bind:value={message} placeholder="Message de validation" aria-label="Message de validation" maxlength="200" />
        <button class="primary" disabled={!message.trim() || busy}>Valider {st.files.length} fichier{st.files.length > 1 ? "s" : ""}</button>
      </form>
    {/if}
  {:else}
    <p class="msg">Chargement…</p>
  {/if}
</div>

<style>
  .git { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  .msg { color: var(--muted); font-size: 13px; padding: 10px 12px; margin: 0; line-height: 1.5; }
  .msg.bad { color: var(--del); }
  code { background: var(--code); padding: 1px 5px; border-radius: 5px; font-size: 12px; }
  .head { display: flex; align-items: center; justify-content: space-between; padding: 8px 10px; border-bottom: 1px solid var(--border); }
  .branch { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; font-weight: 600; }
  .small { border: 1px solid var(--border); background: var(--surface); border-radius: 7px; padding: 3px 8px; font-size: 12.5px; margin: 0 10px; }
  .head .small { margin: 0; display: inline-flex; }
  .acts { display: flex; gap: 4px; }
  .branchform { display: flex; gap: 4px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .branchform input { flex: 1; min-width: 0; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 4px 8px; outline: none; font-size: 13px; }
  .branchform .small { margin: 0; }
  .files { list-style: none; margin: 0; padding: 4px 0; overflow-y: auto; max-height: 38%; flex: none; border-bottom: 1px solid var(--border); }
  .files li button { display: flex; align-items: center; gap: 8px; width: 100%; border: 0; background: none; padding: 4px 10px; text-align: left; font-size: 13px; }
  .files li:hover button, .files li.on button { background: var(--hover); }
  .path { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip { flex: none; width: 18px; height: 18px; display: grid; place-items: center; border-radius: 5px; font-size: 11px; font-weight: 700; background: var(--hover); color: var(--muted); }
  .chip.sM { color: var(--info); }
  .chip.sA, .chip.su { color: var(--add); }
  .chip.sD { color: var(--del); }
  .diff { flex: 1; min-height: 60px; margin: 0; padding: 8px 0; overflow: auto; font-family: ui-monospace, Consolas, monospace; font-size: 12px; line-height: 1.5; }
  .diff span { display: block; padding: 0 10px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .diff .add { background: var(--add-bg); color: var(--add); }
  .diff .del { background: var(--del-bg); color: var(--del); }
  .diff .hunk { color: var(--muted); }
  .commit { display: flex; flex-direction: column; gap: 6px; padding: 8px; border-top: 1px solid var(--border); flex: none; }
  .commit input { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 10px; outline: none; }
  .commit input:focus { border-color: var(--accent); }
  .primary { border: 1px solid var(--accent); background: var(--accent); color: var(--accent-fg); border-radius: 8px; padding: 6px 10px; }
  .primary:disabled { opacity: 0.45; }
</style>
