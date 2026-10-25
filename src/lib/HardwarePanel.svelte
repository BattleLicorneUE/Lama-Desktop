<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import LocalModelsList from "$lib/LocalModelsList.svelte";

  import type { ChatStore } from "$lib/chat.svelte";

  let { chat }: { chat: ChatStore } = $props();

  type Gpu = { name: string; vendor: "nvidia" | "amd" | "intel" | "other"; vram_mb: number; integrated: boolean };
  type Hardware = { ram_mb: number; ram_free_mb: number; cpu: string; cores: number; avx2: boolean; gpus: Gpu[]; disk_free_mb: number };

  let hw = $state<Hardware | null>(null);
  let busy = $state(false);
  let error = $state("");

  async function detect() {
    busy = true;
    error = "";
    try {
      hw = await invoke<Hardware>("hardware_detect");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  onMount(detect);

  const gb = (mb: number) => {
    const v = mb / 1024;
    return `${v >= 100 ? Math.round(v) : v.toFixed(1).replace(".", ",")} Go`;
  };
  const dedicated = $derived(hw?.gpus.find((g) => !g.integrated) ?? null);

  /** Ce que le PC permet, en une phrase. */
  const summary = $derived.by(() => {
    if (!hw) return "";
    if (dedicated && dedicated.vram_mb >= 6000) return `Ta carte graphique (${gb(dedicated.vram_mb)}) permet de faire tourner des modèles locaux de taille moyenne à bonne vitesse.`;
    if (dedicated && dedicated.vram_mb >= 3000) return `Ta carte graphique (${gb(dedicated.vram_mb)}) convient à de petits modèles locaux ; les plus gros seront plus lents.`;
    if (hw.ram_mb >= 12000) return "Pas de carte graphique dédiée utilisable : les modèles locaux tourneront sur le processeur, correctement pour les petits modèles mais lentement pour les autres.";
    return "Ce PC est juste pour des modèles locaux : Ollama Cloud sera bien plus confortable.";
  });
</script>

<section>
  <h3>Ce PC</h3>
  <p class="muted">Ce que ton ordinateur peut faire tourner. Sert à proposer des modèles locaux adaptés ; rien n'est envoyé.</p>

  {#if hw}
    <dl>
      <dt>Processeur</dt><dd>{hw.cpu} · {hw.cores} threads{hw.avx2 ? "" : " · sans AVX2 (lent)"}</dd>
      <dt>Mémoire</dt><dd>{gb(hw.ram_mb)} <span class="muted">({gb(hw.ram_free_mb)} libres)</span></dd>
      <dt>{hw.gpus.length > 1 ? "Cartes graphiques" : "Carte graphique"}</dt>
      <dd>
        {#each hw.gpus as g (g.name)}
          <div>{g.name} <span class="muted">· {g.integrated ? "intégrée, utilise la mémoire du PC" : g.vram_mb ? `${gb(g.vram_mb)} dédiés` : "mémoire inconnue"}</span></div>
        {:else}
          <span class="muted">Aucune détectée</span>
        {/each}
      </dd>
      <dt>Disque</dt><dd>{gb(hw.disk_free_mb)} <span class="muted">libres</span></dd>
    </dl>
    <p class="sum">{summary}</p>
  {:else if busy}
    <p class="muted">Analyse en cours…</p>
  {/if}
  {#if error}<p class="bad" role="alert">{error}</p>{/if}
  <div class="row"><button onclick={detect} disabled={busy}>Analyser de nouveau</button></div>
</section>

{#if hw}<LocalModelsList {chat} hardware={hw} />{/if}

<style>
  section { display: flex; flex-direction: column; gap: 10px; margin-top: 28px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 8px 16px; margin: 0; font-size: 14px; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .sum { margin: 0; padding: 10px 12px; border: 1px solid var(--border); background: var(--surface); border-radius: 10px; font-size: 13.5px; line-height: 1.45; }
  .bad { margin: 0; font-size: 13.5px; color: var(--del); word-break: break-word; }
  .row { display: flex; gap: 8px; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); font: inherit; }
  button:disabled { opacity: 0.45; }
</style>
