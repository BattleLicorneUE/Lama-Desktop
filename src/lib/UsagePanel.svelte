<script lang="ts">
  import { formatTokens, type ChatStore } from "$lib/chat.svelte";

  let { chat }: { chat: ChatStore } = $props();

  const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  // Les 14 derniers jours, du plus ancien au plus récent.
  const days = $derived.by(() => {
    const out: { key: string; label: string; i: number; o: number }[] = [];
    for (let n = 13; n >= 0; n--) {
      const d = new Date();
      d.setDate(d.getDate() - n);
      const key = iso(d);
      const v = chat.usage.days[key];
      out.push({ key, label: `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}`, i: v?.i ?? 0, o: v?.o ?? 0 });
    }
    return out;
  });
  const peak = $derived(Math.max(1, ...days.map((d) => d.i + d.o)));
  const models = $derived(
    Object.entries(chat.usage.models)
      .map(([name, v]) => ({ name, ...v, total: v.i + v.o }))
      .sort((a, b) => b.total - a.total),
  );
  const modelPeak = $derived(Math.max(1, ...models.map((m) => m.total)));
  const last14 = $derived(days.reduce((n, d) => n + d.i + d.o, 0));
</script>

<section>
  
  <p class="muted">Les jetons sont les petits morceaux de texte que le modèle lit et écrit. Ici, ceux de ce profil, comptés à chaque réponse : d'abord ceux que tu envoies, puis ceux que le modèle génère.
  Ollama Cloud a ses propres limites sur ton compte, tu les vois sur <b>ollama.com</b>.</p>
  <div class="cards">
    <div><small>Aujourd'hui</small><b>{formatTokens(chat.usage.dayIn + chat.usage.dayOut)}</b><span>{formatTokens(chat.usage.dayIn)} envoyés · {formatTokens(chat.usage.dayOut)} générés</span></div>
    <div><small>14 derniers jours</small><b>{formatTokens(last14)}</b><span>{formatTokens(days.reduce((n, d) => n + d.i, 0))} envoyés · {formatTokens(days.reduce((n, d) => n + d.o, 0))} générés</span></div>
    <div><small>Depuis le début</small><b>{formatTokens(chat.usage.totalIn + chat.usage.totalOut)}</b><span>{formatTokens(chat.usage.totalIn)} envoyés · {formatTokens(chat.usage.totalOut)} générés</span></div>
  </div>
</section>

<section>
  <h3>Par jour</h3>
  <div class="bars" role="img" aria-label="Jetons par jour sur les 14 derniers jours">
    {#each days as d (d.key)}
      <div class="col" title="{d.label} : {formatTokens(d.i + d.o)} jetons">
        <div class="bar"><span class="o" style:height="{(d.o / peak) * 100}%"></span><span class="i" style:height="{(d.i / peak) * 100}%"></span></div>
        <small>{d.label.slice(0, 2)}</small>
      </div>
    {/each}
  </div>
  <p class="legend"><i class="sw i"></i> envoyés <i class="sw o"></i> générés</p>
</section>

<section>
  <h3>Par modèle</h3>
  {#if models.length}
    <ul>
      {#each models as m (m.name)}
        <li>
          <span class="name">{m.name}</span>
          <span class="track"><span style:width="{(m.total / modelPeak) * 100}%"></span></span>
          <span class="n">{formatTokens(m.total)}</span>
        </li>
      {/each}
    </ul>
    <p class="muted">Le détail par modèle ne compte qu'à partir de cette version.</p>
  {:else}
    <p class="muted">Rien pour l'instant. Les modèles apparaissent après leurs premières réponses.</p>
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 12px; }
  .cards div { display: grid; gap: 3px; border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 14px 16px; }
  .cards small { color: var(--muted); font-size: 12.5px; }
  .cards b { font-size: 24px; font-weight: 500; }
  .cards span { color: var(--muted); font-size: 12.5px; }
  .bars { display: grid; grid-template-columns: repeat(14, 1fr); gap: 6px; align-items: end; height: 150px; }
  .col { display: flex; flex-direction: column; align-items: center; gap: 4px; height: 100%; }
  .bar { flex: 1; width: 100%; display: flex; flex-direction: column; justify-content: flex-end; }
  .bar span { display: block; width: 100%; min-height: 0; }
  .bar .i { background: var(--accent); border-radius: 4px 4px 0 0; order: 1; }
  .bar .o { background: color-mix(in srgb, var(--accent) 40%, var(--bg)); order: 2; }
  .col small { color: var(--muted); font-size: 12px; }
  .legend { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12.5px; margin: 0; }
  .sw { display: inline-block; width: 10px; height: 10px; border-radius: 3px; }
  .sw.i { background: var(--accent); }
  .sw.o { background: color-mix(in srgb, var(--accent) 40%, var(--bg)); margin-left: 10px; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  li { display: grid; grid-template-columns: minmax(120px, 220px) 1fr 70px; align-items: center; gap: 12px; font-size: 14px; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .track { height: 8px; background: var(--hover); border-radius: 6px; overflow: hidden; }
  .track span { display: block; height: 100%; background: var(--accent); border-radius: 6px; }
  .n { text-align: right; color: var(--muted); }
</style>
