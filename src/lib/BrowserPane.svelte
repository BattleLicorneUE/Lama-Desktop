<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onDestroy, onMount, untrack } from "svelte";
  import Icon from "$lib/Icon.svelte";
  import { pretty, searchUrl, toAddress } from "$lib/browser";

  let { url, nonce, onclose }: { url: string; nonce: number; onclose: () => void } = $props();

  /** La vraie page web (WebView2) est posée par le cœur Rust sur le cadre vide ci-dessous. */
  let frame = $state<HTMLDivElement>();
  let address = $state("");
  let shown = $state("");
  let title = $state("");
  let loading = $state(true);
  let error = $state("");
  let editing = false;
  let ready = false;
  let hidden = false;
  let lastRect = "";
  let lastNonce = untrack(() => nonce);

  const owner = (() => {
    try {
      return getCurrentWindow().label;
    } catch {
      return "main";
    }
  })();

  function rect() {
    const r = frame!.getBoundingClientRect();
    return { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) };
  }

  async function open(target: string) {
    error = "";
    loading = true;
    const r = rect();
    lastRect = JSON.stringify(r);
    try {
      await invoke("webpane_open", { url: target, ...r });
      ready = true;
    } catch (e) {
      loading = false;
      error = String(e);
    }
  }

  async function go(target: string) {
    error = "";
    if (!ready) return open(target);
    loading = true;
    try {
      await invoke("webpane_navigate", { url: target });
    } catch (e) {
      loading = false;
      error = String(e);
    }
  }

  const act = (action: "back" | "forward" | "reload" | "stop") => invoke("webpane_action", { action }).catch(() => {});

  function submit(e: Event) {
    e.preventDefault();
    editing = false;
    void go(toAddress(address));
  }

  /** Une boîte de dialogue de l'application doit passer devant : la page native, elle, est toujours au-dessus. */
  const modalOpen = () => !!document.querySelector('[role="dialog"], [role="alertdialog"], [aria-modal="true"]');

  onMount(() => {
    // La page native passe devant l'interface : les notifications se décalent pour rester visibles (voir UpdateToast et Celebrate).
    document.body.dataset.pane = "right";
    shown = url;
    address = pretty(url);
    void open(url);
    const un = listen<{ window: string; url?: string | null; title?: string | null; loading?: boolean | null }>("webpane:nav", (e) => {
      const p = e.payload;
      if (p.window !== owner) return;
      if (p.url) {
        shown = p.url;
        if (!editing) address = pretty(p.url);
      }
      if (p.title != null) title = p.title;
      if (p.loading != null) loading = p.loading;
    });
    // Le panneau peut bouger sans changer de taille (barre latérale, fenêtre) : on compare son rectangle plusieurs fois par seconde.
    const tick = setInterval(() => {
      if (!frame || !ready) return;
      const wantHidden = modalOpen();
      if (wantHidden !== hidden) {
        hidden = wantHidden;
        void invoke("webpane_visible", { visible: !hidden }).catch(() => {});
      }
      const r = rect();
      const key = JSON.stringify(r);
      if (key !== lastRect) {
        lastRect = key;
        void invoke("webpane_bounds", r).catch(() => {});
      }
    }, 120);
    return () => {
      delete document.body.dataset.pane;
      clearInterval(tick);
      un.then((f) => f());
    };
  });

  onDestroy(() => {
    void invoke("webpane_close").catch(() => {});
  });

  $effect(() => {
    if (nonce !== lastNonce) {
      lastNonce = nonce;
      shown = url;
      address = pretty(url);
      void go(url);
    }
  });
</script>

<aside aria-label="Navigateur">
  <div class="bar">
    <button onclick={() => act("back")} aria-label="Page précédente" title="Page précédente"><Icon name="chevron-left" size={16} /></button>
    <button onclick={() => act("forward")} aria-label="Page suivante" title="Page suivante"><Icon name="chevron-right" size={16} /></button>
    <button onclick={() => act(loading ? "stop" : "reload")} aria-label={loading ? "Arrêter le chargement" : "Recharger la page"} title={loading ? "Arrêter" : "Recharger"}>
      <Icon name={loading ? "close" : "refresh"} size={16} />
    </button>
    <form onsubmit={submit}>
      <input
        bind:value={address}
        onfocus={(e) => {
          editing = true;
          e.currentTarget.select();
        }}
        onblur={() => {
          editing = false;
          address = pretty(shown);
        }}
        spellcheck="false"
        autocomplete="off"
        aria-label="Adresse ou recherche"
        placeholder="Adresse ou recherche"
      />
    </form>
    <button onclick={() => openUrl(shown).catch(() => {})} aria-label="Ouvrir dans le navigateur par défaut" title="Ouvrir dans le navigateur par défaut" disabled={!/^https?:/.test(shown)}><Icon name="external" size={16} /></button>
    <button class="x" onclick={onclose} aria-label="Fermer le navigateur" title="Fermer"><Icon name="close" size={17} /></button>
  </div>
  {#if loading}<div class="progress" role="progressbar" aria-label="Chargement de la page"></div>{/if}
  <div class="frame" bind:this={frame}>
    {#if error}
      <div class="msg" role="alert">
        <p><b>Impossible d'ouvrir cette page.</b></p>
        <p>{error}</p>
        <button onclick={() => go(searchUrl(address))}>Chercher « {address.slice(0, 40)} » à la place</button>
      </div>
    {:else}
      <p class="wait">{title || "Chargement…"}</p>
    {/if}
  </div>
  <p class="foot" title={pretty(shown)}>{title || pretty(shown)}</p>
</aside>

<style>
  aside { width: 46%; min-width: 340px; border-left: 1px solid var(--border); display: flex; flex-direction: column; background: var(--bg); position: relative; }
  .bar { display: flex; align-items: center; gap: 4px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .bar button { display: grid; place-items: center; width: 30px; height: 30px; border: 0; background: none; color: var(--muted); border-radius: 8px; flex: none; }
  .bar button:hover:not(:disabled) { background: var(--hover); color: var(--fg); }
  .bar button:disabled { opacity: 0.35; }
  form { flex: 1; min-width: 0; }
  input { width: 100%; height: 30px; padding: 0 12px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface); color: var(--fg); font: inherit; font-size: 13px; outline: none; }
  input:focus-visible { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  .progress { position: absolute; top: 43px; left: 0; right: 0; height: 2px; background: linear-gradient(90deg, transparent, var(--accent), transparent); background-size: 40% 100%; background-repeat: no-repeat; animation: slide 1.1s linear infinite; z-index: 1; }
  @keyframes slide { from { background-position: -40% 0; } to { background-position: 140% 0; } }
  @media (prefers-reduced-motion: reduce) { .progress { animation: none; background: var(--accent); opacity: 0.5; } }
  .frame { flex: 1; min-height: 0; display: grid; place-items: center; background: var(--surface); }
  .wait { color: var(--muted); font-size: 13px; margin: 0; padding: 0 24px; text-align: center; }
  .msg { max-width: 340px; padding: 24px; text-align: center; display: grid; gap: 8px; }
  .msg p { margin: 0; color: var(--muted); font-size: 13.5px; overflow-wrap: anywhere; }
  .msg b { color: var(--fg); }
  .msg button { justify-self: center; margin-top: 6px; border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 8px; padding: 6px 12px; font-size: 13px; }
  .foot { margin: 0; padding: 4px 12px; border-top: 1px solid var(--border); color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
