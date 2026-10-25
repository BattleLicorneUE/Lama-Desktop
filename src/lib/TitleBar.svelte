<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import Logo from "$lib/Logo.svelte";

  let { title = "Lama Desktop" }: { title?: string } = $props();

  let maximized = $state(false);

  /** Hors de la fenêtre Tauri (aperçu navigateur), ces appels échouent sans importance. */
  const win = () => {
    try {
      return getCurrentWindow();
    } catch {
      return null;
    }
  };

  async function refresh() {
    maximized = (await win()?.isMaximized().catch(() => false)) ?? false;
  }

  onMount(() => {
    refresh();
    const un = win()?.onResized(refresh).catch(() => null);
    return () => {
      un?.then((f) => f?.());
    };
  });
</script>

<header class="bar" data-tauri-drag-region>
  <span class="brand" data-tauri-drag-region><Logo size={16} /> <span data-tauri-drag-region>{title}</span></span>
  <div class="controls">
    <button onclick={() => win()?.minimize()} aria-label="Réduire" title="Réduire">
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M2 6h8" stroke="currentColor" stroke-width="1.2" fill="none" /></svg>
    </button>
    <button onclick={() => win()?.toggleMaximize()} aria-label={maximized ? "Restaurer" : "Agrandir"} title={maximized ? "Restaurer" : "Agrandir"}>
      {#if maximized}
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M4 2.5h5.5V8M2.5 4H8v5.5H2.5z" stroke="currentColor" stroke-width="1.1" fill="none" stroke-linejoin="round" /></svg>
      {:else}
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><rect x="2.5" y="2.5" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.2" fill="none" /></svg>
      {/if}
    </button>
    <button class="close" onclick={() => win()?.close()} aria-label="Fermer" title="Fermer">
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.2" fill="none" stroke-linecap="round" /></svg>
    </button>
  </div>
</header>

<style>
  .bar { display: flex; align-items: center; justify-content: space-between; height: 34px; flex: none; background: var(--sidebar); border-bottom: 1px solid var(--border); user-select: none; -webkit-user-select: none; }
  .brand { display: flex; align-items: center; gap: 8px; padding-left: 12px; font-size: 12.5px; color: var(--muted); }
  .controls { display: flex; height: 100%; }
  .controls button { display: grid; place-items: center; width: 46px; height: 100%; border: 0; background: none; color: var(--muted); border-radius: 0; }
  .controls button:hover { background: var(--hover); color: var(--fg); }
  .controls .close:hover { background: #c42b1c; color: #fff; }
</style>
