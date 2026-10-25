<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "$lib/Icon.svelte";

  type Item = { value: string; label: string };
  let { items, value, onchange, title, disabled = false, up = false, flat = false, placeholder = "Choisir", trigger }: {
    items: Item[];
    value: string;
    onchange: (value: string) => void;
    title?: string;
    disabled?: boolean;
    /** Ouvre la liste vers le haut (zone de saisie en bas de l'écran). */
    up?: boolean;
    /** Style discret (sans bordure), comme les menus Mode et Effort. */
    flat?: boolean;
    placeholder?: string;
    /** Remplace le bouton par défaut (ex. une simple icône). */
    trigger?: Snippet<[boolean]>;
  } = $props();

  let open = $state(false);
  let query = $state("");
  let cursor = $state(0);
  let root: HTMLElement;
  let search = $state<HTMLInputElement>();

  const current = $derived(items.find((i) => i.value === value));
  const shown = $derived(items.filter((i) => i.label.toLowerCase().includes(query.trim().toLowerCase())));
  const searchable = $derived(items.length > 8);

  function toggle() {
    if (disabled) return;
    open = !open;
    query = "";
    cursor = Math.max(0, items.findIndex((i) => i.value === value));
  }

  function pick(v: string) {
    open = false;
    if (v !== value) onchange(v);
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") open = false;
    else if (e.key === "ArrowDown") cursor = Math.min(shown.length - 1, cursor + 1);
    else if (e.key === "ArrowUp") cursor = Math.max(0, cursor - 1);
    else if (e.key === "Enter" && shown[cursor]) pick(shown[cursor].value);
    else return;
    e.preventDefault();
    queueMicrotask(() => root?.querySelector(".opt.cur")?.scrollIntoView({ block: "nearest" }));
  }

  $effect(() => {
    if (open && searchable) search?.focus();
  });
</script>

<svelte:window
  onmousedown={(e) => open && !root.contains(e.target as Node) && (open = false)}
  onkeydown={onKey}
/>

<div class="select" bind:this={root}>
  <button type="button" class="trigger" class:plain={!!trigger} class:flat onclick={toggle} {disabled} {title} aria-haspopup="listbox" aria-expanded={open}>
    {#if trigger}
      {@render trigger(open)}
    {:else}
      <span class="label">{current?.label ?? placeholder}</span><Icon name="chevron" size={13} />
    {/if}
  </button>

  {#if open}
    <div class="pop" class:up role="listbox">
      {#if searchable}
        <input bind:this={search} bind:value={query} oninput={() => (cursor = 0)} placeholder="Rechercher…" aria-label="Rechercher" spellcheck="false" />
      {/if}
      <div class="list">
        {#each shown as it, i (it.value)}
          <button type="button" class="opt" class:cur={i === cursor} class:sel={it.value === value} role="option" aria-selected={it.value === value}
            onmouseenter={() => (cursor = i)} onclick={() => pick(it.value)}>
            <span class="txt">{it.label}</span>
            {#if it.value === value}<span class="check"><Icon name="check" size={14} /></span>{/if}
          </button>
        {:else}
          <p class="none">Aucun résultat</p>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .select { position: relative; display: inline-block; min-width: 0; }
  .trigger { display: flex; align-items: center; gap: 6px; max-width: 100%; border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 9px; padding: 6px 10px; font-size: 14px; text-align: left; }
  .trigger:hover:not(:disabled) { border-color: var(--accent); }
  .trigger:disabled { opacity: 0.55; }
  .trigger.flat { border: 0; background: none; color: var(--muted); padding: 5px 8px; max-width: 220px; }
  .trigger.flat:hover:not(:disabled) { background: var(--hover); color: var(--fg); }
  .trigger.plain { border: 0; background: none; padding: 0; }
  .label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pop { position: absolute; top: calc(100% + 6px); left: 0; z-index: 30; min-width: 240px; max-width: 340px; background: var(--surface); border: 1px solid var(--border); border-radius: 14px; padding: 6px; box-shadow: 0 10px 30px rgb(0 0 0 / 0.18); }
  .pop.up { top: auto; bottom: calc(100% + 8px); }
  input { width: 100%; border: 1px solid var(--border); background: var(--bg); border-radius: 9px; padding: 6px 10px; margin-bottom: 6px; outline: none; font-size: 14px; }
  input:focus { border-color: var(--accent); }
  .list { max-height: 300px; overflow-y: auto; display: flex; flex-direction: column; gap: 1px; }
  .opt { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; border: 0; background: none; padding: 7px 10px; border-radius: 8px; font-size: 14px; }
  .opt.cur { background: var(--hover); }
  .opt.sel { font-weight: 600; }
  .txt { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .check { color: var(--info); display: inline-flex; }
  .none { margin: 0; padding: 8px 10px; color: var(--muted); font-size: 13px; }
</style>
