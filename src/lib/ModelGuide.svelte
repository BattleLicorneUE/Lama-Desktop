<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { ChatStore } from "$lib/chat.svelte";
  import { BANDS, GUIDE_DATE, MODELS_URL, PRICING_URL, USES, forUse, guideFor, priceText, priceTier, type GuideModel, type Use } from "$lib/models-guide";

  let { chat, onpick }: { chat: ChatStore; onpick?: (name: string) => void } = $props();

  let use = $state<Use | "all">("all");
  const list = $derived(forUse(use));
  const hint = $derived(USES.find((u) => u.id === use)?.hint ?? "Tous les modèles du guide.");

  /** Nom exact chez Ollama pour ce modèle sur ton compte (la fiche peut être « gemma4 » alors que le compte propose « gemma4:31b »). */
  function available(g: GuideModel): string | undefined {
    const mine = chat.models.map((m) => m.name);
    return mine.find((n) => n.toLowerCase().replace(/-cloud$/, "") === g.id) ?? mine.find((n) => guideFor(n)?.id === g.id);
  }

  const others = $derived(chat.models.filter((m) => !guideFor(m.name)));
  const connected = $derived(chat.models.length > 0);

  function choose(name: string) {
    chat.setModel(name);
    onpick?.(name);
  }
</script>

<div class="guide">
  <div class="chips" role="group" aria-label="Choisir un usage">
    <button class="chip" class:on={use === "all"} onclick={() => (use = "all")}>Tous</button>
    {#each USES as u (u.id)}
      <button class="chip" class:on={use === u.id} onclick={() => (use = u.id)}>{u.label}</button>
    {/each}
  </div>
  <p class="hint">{hint} Du moins cher au plus cher.</p>

  <ul>
    {#each list as g (g.id)}
      {@const mine = available(g)}
      {@const tier = priceTier(g)}
      <li class:current={mine === chat.model}>
        <div class="top">
          <b>{g.id}</b>
          <span class="maker">{g.maker}</span>
          {#if g.pick}<span class="pick">{g.pick}</span>{/if}
          <span class="grow"></span>
          <span class="price" title={tier.label}>{tier.symbols}</span>
        </div>
        <p class="blurb">{g.blurb}</p>
        <div class="meta">
          <span title={BANDS[g.band].hint}>{BANDS[g.band].label}</span>
          <span>{g.ctx} de contexte</span>
          {#if g.caps.includes("vision")}<span>Images</span>{/if}
          {#if g.caps.includes("tools")}<span>Outils</span>{/if}
          {#if g.caps.includes("thinking")}<span>Réflexion</span>{/if}
        </div>
        <div class="foot">
          <span class="cost">{priceText(g)}</span>
          {#if mine}
            <button class="use" onclick={() => choose(mine)} disabled={mine === chat.model || chat.busy}>{mine === chat.model ? "Utilisé" : "Utiliser"}</button>
          {:else if connected}
            <span class="na" title="Ton compte ne propose pas ce modèle (plan ou crédits).">Non disponible sur ton compte</span>
          {/if}
        </div>
      </li>
    {/each}
  </ul>

  {#if others.length}
    <p class="hint">Autres modèles de ton compte, absents du guide : {others.map((m) => m.name).join(", ")}.</p>
  {/if}

  <p class="src">
    Prix et niveaux d'usage relevés le {GUIDE_DATE} sur
    <button class="link" onclick={() => openUrl(PRICING_URL).catch(() => {})}>ollama.com/pricing</button> et
    <button class="link" onclick={() => openUrl(MODELS_URL).catch(() => {})}>la liste des modèles</button> : ils changent, vérifie-les avant de t'abonner.
    « $ » = très économique, « $$$$ » = très cher (d'après le prix des jetons générés).
  </p>
</div>

<style>
  .guide { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 3px 12px; font-size: 13px; color: var(--fg); }
  .chip.on { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .hint, .src { margin: 0; color: var(--muted); font-size: 13px; line-height: 1.45; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  li { border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 11px 13px; display: grid; gap: 5px; }
  li.current { border-color: var(--accent); }
  .top { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .top b { font-weight: 600; font-size: 14.5px; }
  .maker { color: var(--muted); font-size: 12.5px; }
  .pick { background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); border-radius: 6px; padding: 1px 8px; font-size: 12px; }
  .grow { flex: 1; }
  .price { font-weight: 600; letter-spacing: 0.03em; }
  .blurb { margin: 0; font-size: 13.5px; line-height: 1.4; }
  .meta { display: flex; gap: 6px; flex-wrap: wrap; }
  .meta span { background: var(--hover); color: var(--muted); border-radius: 6px; padding: 1px 7px; font-size: 12px; }
  .foot { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .cost { color: var(--muted); font-size: 12.5px; }
  .na { color: var(--muted); font-size: 12.5px; font-style: italic; }
  .use { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 3px 12px; color: var(--fg); }
  .use:hover:not(:disabled) { border-color: var(--accent); }
  .use:disabled { opacity: 0.5; }
  .link { border: 0; background: none; color: var(--accent); padding: 0; text-decoration: underline; font: inherit; }
</style>
