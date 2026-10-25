<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type Hardware = Record<string, unknown>;
  type Item = {
    id: string;
    name: string;
    params_b: number;
    size_mb: number;
    uses: string[];
    tools: boolean;
    vision: boolean;
    blurb: string;
    level: "recommended" | "possible" | "heavy";
    mode: "gpu" | "mixed" | "cpu" | null;
    speed: "fast" | "medium" | "slow" | null;
    reason: string;
    disk_ok: boolean;
    need_mb: number;
    best: boolean;
  };

  let { hardware }: { hardware: Hardware } = $props();
  let items = $state<Item[]>([]);
  let error = $state("");
  let showHeavy = $state(false);

  $effect(() => {
    invoke<Item[]>("local_models_assess", { hardware })
      .then((r) => (items = r))
      .catch((e) => (error = String(e)));
  });

  const gb = (mb: number) => `${(mb / 1024).toFixed(1).replace(".", ",")} Go`;
  const USE: Record<string, string> = { chat: "discussion", code: "code", reasoning: "raisonnement" };
  const SPEED = { fast: "Rapide", medium: "Correct", slow: "Lent" } as const;
  const MODE = { gpu: "carte graphique", mixed: "carte graphique + mémoire du PC", cpu: "processeur" } as const;

  const good = $derived(items.filter((i) => i.level !== "heavy"));
  const heavy = $derived(items.filter((i) => i.level === "heavy"));
</script>

<section>
  <h3>Modèles locaux pour ce PC</h3>
  <p class="muted">
    Des modèles qui tournent sur ton ordinateur, sans internet ni clé API. Les tailles sont approximatives et la vitesse dépend de ton matériel.
    <b>Le téléchargement n'est pas encore disponible.</b>
  </p>

  {#if error}<p class="bad" role="alert">{error}</p>{/if}

  {#each good as m (m.id)}
    <article class:best={m.best}>
      <header>
        <b>{m.name}</b>
        {#if m.best}<span class="tag star">Conseillé pour ton PC</span>{/if}
        {#if m.speed}<span class="tag" class:fast={m.speed === "fast"} class:slow={m.speed === "slow"}>{SPEED[m.speed]}</span>{/if}
        <span class="size">{gb(m.size_mb)}</span>
      </header>
      <p class="blurb">{m.blurb}</p>
      <div class="meta">
        {#each m.uses as u}<span class="tag soft">{USE[u] ?? u}</span>{/each}
        {#if m.vision}<span class="tag soft">images</span>{/if}
        <span class="tag soft" class:no={!m.tools}>{m.tools ? "outils de l'agent" : "sans outils de l'agent"}</span>
      </div>
      <p class="why">{m.mode ? `Sur : ${MODE[m.mode]}. ` : ""}{m.reason}{m.disk_ok ? "" : " Pas assez de place sur le disque."}</p>
    </article>
  {/each}

  {#if heavy.length}
    <button class="link" onclick={() => (showHeavy = !showHeavy)}>{showHeavy ? "Masquer" : "Voir"} les {heavy.length} modèles trop lourds pour ce PC</button>
    {#if showHeavy}
      {#each heavy as m (m.id)}
        <article class="off">
          <header><b>{m.name}</b><span class="size">{gb(m.size_mb)}</span></header>
          <p class="why">{m.reason}</p>
        </article>
      {/each}
    {/if}
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; margin-top: 28px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .bad { margin: 0; font-size: 13.5px; color: var(--del); }
  article { border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 12px 14px; display: flex; flex-direction: column; gap: 6px; }
  article.best { border-color: var(--fg); }
  article.off { opacity: 0.6; }
  header { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .size { margin-left: auto; color: var(--muted); font-size: 13px; }
  .blurb { margin: 0; font-size: 13.5px; line-height: 1.45; }
  .why { margin: 0; font-size: 12.5px; color: var(--muted); line-height: 1.4; }
  .meta { display: flex; gap: 6px; flex-wrap: wrap; }
  .tag { font-size: 11.5px; padding: 2px 8px; border-radius: 999px; border: 1px solid var(--border); background: var(--code); }
  .tag.soft { color: var(--muted); }
  .tag.no { opacity: 0.7; text-decoration: line-through; text-decoration-thickness: 1px; }
  .tag.fast { color: var(--add); border-color: color-mix(in srgb, var(--add) 50%, var(--border)); }
  .tag.slow { color: var(--del); border-color: color-mix(in srgb, var(--del) 50%, var(--border)); }
  .tag.star { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  .link { align-self: flex-start; border: none; background: none; color: var(--muted); font: inherit; font-size: 13px; text-decoration: underline; padding: 2px 0; cursor: pointer; }
</style>
