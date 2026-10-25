<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import Self from "$lib/FileTree.svelte";
  import Icon from "$lib/Icon.svelte";
  import type { FsCtl } from "$lib/fs-ctl.svelte";

  type Entry = { name: string; is_dir: boolean };
  let { workspace, ctl, rel = "", depth = 0, refreshKey = 0 }: {
    workspace: string;
    ctl: FsCtl;
    rel?: string;
    depth?: number;
    refreshKey?: number;
  } = $props();

  let entries = $state<Entry[]>([]);
  let error = $state("");
  let expanded = $state<Record<string, boolean>>({});

  async function load() {
    try {
      entries = await invoke<Entry[]>("fs_list", { workspace, rel: rel || "." });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  onMount(load);
  $effect(() => {
    workspace;
    refreshKey;
    load();
  });

  // Une création dans un dossier replié le déplie pour montrer le champ de saisie.
  $effect(() => {
    const d = ctl.draft;
    if (d && d.mode !== "rename" && d.path && entries.some((e) => e.is_dir && pathOf(e) === d.path)) expanded[d.path] = true;
  });

  const pathOf = (e: Entry) => (rel ? `${rel}/${e.name}` : e.name);
  const pad = (extra = 0) => `${8 + depth * 14 + extra}px`;

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") ctl.commit();
    else if (e.key === "Escape") ctl.cancel();
  }
</script>

{#snippet editor(extra: number)}
  {#if ctl.draft}
    <div class="edit" style:padding-left={pad(extra)}>
      <input
        use:focusSelect
        bind:value={ctl.draft.value}
        onkeydown={onKey}
        onblur={() => !ctl.error && ctl.cancel()}
        placeholder={ctl.draft.mode === "dir" ? "Nom du dossier" : "Nom du fichier"}
        aria-label={ctl.draft.mode === "rename" ? "Nouveau nom" : ctl.draft.mode === "dir" ? "Nom du nouveau dossier" : "Nom du nouveau fichier"}
        spellcheck="false"
      />
      {#if ctl.error}<p class="err">{ctl.error}</p>{/if}
    </div>
  {/if}
{/snippet}

{#if error}
  <p class="err" style:padding-left={pad()}>{error}</p>
{:else}
  <ul>
    {#if ctl.draft && ctl.draft.mode !== "rename" && ctl.draft.path === rel}
      <li>{@render editor(14)}</li>
    {/if}
    {#each entries as e (e.name)}
      {@const path = pathOf(e)}
      <li>
        {#if ctl.draft?.mode === "rename" && ctl.draft.path === path}
          {@render editor(e.is_dir ? 0 : 14)}
        {:else}
          <div class="row">
            {#if e.is_dir}
              <button class="name" style:padding-left={pad()} onclick={() => (expanded[path] = !expanded[path])} aria-expanded={!!expanded[path]}>
                <span class="arrow">{expanded[path] ? "▾" : "▸"}</span> {e.name}
              </button>
            {:else}
              <div class="name file" style:padding-left={pad(14)}>{e.name}</div>
            {/if}
            <span class="tools">
              {#if e.is_dir}
                <button title="Nouveau fichier" aria-label="Nouveau fichier dans {e.name}" onclick={() => ctl.start("file", path)}><Icon name="file" size={13} /></button>
                <button title="Nouveau dossier" aria-label="Nouveau dossier dans {e.name}" onclick={() => ctl.start("dir", path)}><Icon name="folder" size={13} /></button>
              {/if}
              <button title="Renommer" aria-label="Renommer {e.name}" onclick={() => ctl.start("rename", path, e.name)}><Icon name="pencil" size={13} /></button>
              <button title="Supprimer" aria-label="Supprimer {e.name}" onclick={() => ctl.remove(path, e.is_dir)}><Icon name="trash" size={13} /></button>
            </span>
          </div>
        {/if}
        {#if e.is_dir && expanded[path]}<Self {workspace} {ctl} rel={path} depth={depth + 1} {refreshKey} />{/if}
      </li>
    {:else}
      {#if !(ctl.draft && ctl.draft.mode !== "rename" && ctl.draft.path === rel)}
        <li class="empty" style:padding-left={pad()}>(vide)</li>
      {/if}
    {/each}
  </ul>
{/if}

<style>
  ul { list-style: none; margin: 0; padding: 0; }
  .row { display: flex; align-items: center; }
  .row:hover, .row:focus-within { background: var(--hover); }
  .name { flex: 1; min-width: 0; display: block; text-align: left; border: 0; background: none; padding: 3px 8px; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arrow { display: inline-block; width: 12px; color: var(--muted); }
  .file { color: var(--muted); }
  .tools { display: none; flex: none; padding-right: 4px; }
  .row:hover .tools, .row:focus-within .tools { display: inline-flex; }
  .tools button { border: 0; background: none; color: var(--muted); padding: 3px 4px; border-radius: 5px; display: inline-flex; }
  .tools button:hover { color: var(--accent); background: var(--surface); }
  .edit { padding-right: 8px; }
  .edit input { width: 100%; border: 1px solid var(--accent); background: var(--surface); border-radius: 6px; padding: 2px 6px; font-size: 13px; outline: none; }
  .empty, .err { font-size: 12px; color: var(--muted); padding: 3px 8px; margin: 0; }
  .err { color: var(--del); }
</style>
