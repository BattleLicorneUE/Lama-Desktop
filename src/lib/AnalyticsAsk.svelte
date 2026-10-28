<script lang="ts">
  import { onMount } from "svelte";
  import { analytics } from "$lib/analytics.svelte";

  let { onmore }: { onmore: () => void } = $props();

  onMount(() => void analytics.load());

  const show = $derived(analytics.info?.available && !analytics.info.asked);
</script>

{#if show}
  <div class="ask" role="dialog" aria-label="Statistiques anonymes">
    <p><b>Un coup de main pour améliorer Lama ?</b><br />
      Envoyer des statistiques anonymes (version, modèles utilisés, fonctions essayées). Jamais le contenu de tes conversations.</p>
    <div class="row">
      <button class="go" onclick={() => analytics.set(true)}>Oui, d'accord</button>
      <button onclick={() => analytics.set(false)}>Non merci</button>
      <button class="link" onclick={onmore}>Voir le détail</button>
    </div>
  </div>
{/if}

<style>
  .ask { position: fixed; left: 16px; bottom: 16px; z-index: 55; max-width: 340px; padding: 12px 14px; border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 12px; box-shadow: 0 8px 28px rgb(0 0 0 / 0.18); font-size: 13.5px; display: flex; flex-direction: column; gap: 10px; }
  p { margin: 0; line-height: 1.45; }
  .row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 4px 10px; color: var(--fg); font: inherit; }
  button.go { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  button.link { border: none; background: none; color: var(--muted); text-decoration: underline; padding: 4px; }
</style>
