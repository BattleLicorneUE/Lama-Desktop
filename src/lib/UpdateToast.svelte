<script lang="ts">
  import { updater } from "$lib/updater.svelte";

  let { onopen }: { onopen: () => void } = $props();
</script>

{#if updater.status === "available" && !updater.dismissed}
  <div class="toast" role="status">
    <span>La version <b>{updater.version}</b> est disponible.</span>
    <button class="go" onclick={onopen}>Voir</button>
    <button class="x" aria-label="Plus tard" onclick={() => (updater.dismissed = true)}>×</button>
  </div>
{/if}

<style>
  .toast { position: fixed; right: 16px; bottom: 16px; z-index: 60; display: flex; align-items: center; gap: 10px; padding: 10px 12px 10px 14px; border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 12px; box-shadow: 0 8px 28px rgb(0 0 0 / 0.18); font-size: 13.5px; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 4px 10px; color: var(--fg); font: inherit; }
  button.go { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  button.x { border: none; background: none; font-size: 18px; line-height: 1; padding: 0 4px; color: var(--muted); }
  :global(body[data-pane]) .toast { right: calc(46% + 16px); }
</style>
