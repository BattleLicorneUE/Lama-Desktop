<script lang="ts">
  import Icon from "$lib/Icon.svelte";
  import type { MenuItem } from "$lib/code-options";

  let { title, icon, items, value, onselect, hint }: {
    title: string;
    icon: string;
    items: MenuItem[];
    value: string;
    onselect: (id: string) => void;
    hint?: string;
  } = $props();

  let open = $state(false);
  let root: HTMLElement;
  const current = $derived(items.find((i) => i.id === value));

  function pick(id: string) {
    onselect(id);
    open = false;
  }

  function onWindowDown(e: MouseEvent) {
    if (open && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onmousedown={onWindowDown} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="menu" bind:this={root}>
  <button class="trigger" onclick={() => (open = !open)} aria-haspopup="listbox" aria-expanded={open} title={title}>
    <Icon name={icon} size={15} />{current?.label}<Icon name="chevron" size={13} />
  </button>

  {#if open}
    <div class="pop" role="listbox">
      <div class="head">{title}</div>
      {#if hint}<p class="hint">{hint}</p>{/if}
      {#each items as it, i}
        <button class="item" role="option" aria-selected={it.id === value} class:sel={it.id === value} onclick={() => pick(it.id)}>
          <span class="top">
            <span class="name" class:accent={it.accent}>{it.label}</span>
            {#if it.badge}<span class="badge">{it.badge}</span>{/if}
            <span class="tail">{#if it.id === value}<span class="check">✓</span>{/if}<span class="num">{i + 1}</span></span>
          </span>
          {#if it.desc}<span class="desc">{it.desc}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu { position: relative; }
  .trigger { display: flex; align-items: center; gap: 5px; border: 0; background: none; color: var(--muted); padding: 5px 8px; border-radius: 8px; font-size: 14px; }
  .trigger:hover { background: var(--hover); color: var(--fg); }
  .pop { position: absolute; bottom: calc(100% + 8px); left: 0; z-index: 20; width: 300px; background: var(--surface); border: 1px solid var(--border); border-radius: 14px; padding: 8px; box-shadow: 0 10px 30px rgb(0 0 0 / 0.18); }
  .head { color: var(--muted); font-size: 13px; padding: 4px 10px 6px; }
  .hint { color: var(--muted); font-size: 13px; line-height: 1.4; margin: 0; padding: 0 10px 8px; }
  .item { display: block; width: 100%; text-align: left; border: 0; background: none; padding: 8px 10px; border-radius: 9px; }
  .item:hover { background: var(--hover); }
  .top { display: flex; align-items: center; gap: 8px; }
  .name { font-weight: 600; }
  .name.accent { color: var(--violet); }
  .badge { font-size: 12px; background: var(--hover); color: var(--muted); border-radius: 6px; padding: 1px 7px; }
  .tail { margin-left: auto; display: flex; gap: 12px; color: var(--muted); font-size: 13px; }
  .check { color: var(--info); }
  .desc { display: block; color: var(--muted); font-size: 13px; margin-top: 2px; line-height: 1.35; }
</style>
