<script lang="ts">
  import { onMount } from "svelte";
  import type { ChatStore } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";
  import Icon from "$lib/Icon.svelte";
  import { BADGES, stepOf, titleOf } from "$lib/progress";

  let { chat }: { chat: ChatStore } = $props();

  const st = $derived(stepOf(chat.progress.xp));
  const got = $derived(BADGES.filter((b) => chat.progress.badges[b.id]).length);
  const R = 54;
  const C = 2 * Math.PI * R;
  // L'anneau se remplit à l'ouverture de la page.
  let shown = $state(0);
  onMount(() => {
    const t = requestAnimationFrame(() => requestAnimationFrame(() => (shown = 1)));
    return () => cancelAnimationFrame(t);
  });

  const STATS = $derived([
    { k: "Messages envoyés", v: chat.progress.counts.messages },
    { k: "Réponses reçues", v: chat.progress.counts.answers },
    { k: "Tâches de code", v: chat.progress.counts.code },
    { k: "Projets créés", v: chat.progress.counts.projects },
    { k: "Souvenirs ajoutés", v: chat.progress.counts.memories },
    { k: "Modèles essayés", v: chat.progress.models.length },
  ]);

  const dateLabel = (d: string) => (d ? new Date(d + "T12:00:00").toLocaleDateString("fr-FR", { day: "numeric", month: "short" }) : "");

  async function reset() {
    if (await askConfirm("Remettre à zéro tes points, ta série et tes badges ? Tes conversations ne sont pas touchées.", "Réinitialiser")) chat.resetProgress();
  }
</script>

<section>
  
  <p class="muted">Tu gagnes des points en utilisant Lama, tu montes de niveau et tu débloques des badges. Tout reste sur ce PC, pour ce profil.</p>
  <label class="check">
    <input type="checkbox" checked={chat.gamify} onchange={(e) => chat.setGamify(e.currentTarget.checked)} />
    Afficher la progression et les félicitations
  </label>
</section>

{#if chat.gamify}
  <section class="hero">
    <div class="ring">
      <svg viewBox="0 0 130 130" width="130" height="130" aria-hidden="true">
        <circle cx="65" cy="65" r={R} class="track" />
        <circle cx="65" cy="65" r={R} class="bar" style:stroke-dasharray={C} style:stroke-dashoffset={C * (1 - st.ratio * shown)} />
      </svg>
      <div class="lv"><small>Niveau</small><b>{st.level}</b></div>
    </div>
    <div class="info">
      <h4>{titleOf(st.level)}</h4>
      <p class="muted">{st.into} / {st.need} points pour le niveau {st.level + 1}</p>
      <div class="chips">
        <span class="chip" class:hot={chat.progress.streak >= 2}><Icon name="flame" size={15} /> {chat.progress.streak} jour{chat.progress.streak > 1 ? "s" : ""} de suite</span>
        <span class="chip">Record : {chat.progress.best}</span>
        <span class="chip"><Icon name="trophy" size={15} /> {got} / {BADGES.length} badges</span>
      </div>
    </div>
  </section>

  <section>
    <h3>En chiffres</h3>
    <div class="stats">
      {#each STATS as s, i}<div style:--i={i}><b>{s.v}</b><span>{s.k}</span></div>{/each}
    </div>
  </section>

  <section>
    <h3>Badges</h3>
    <div class="badges">
      {#each BADGES as b, i (b.id)}
        {@const date = chat.progress.badges[b.id]}
        <div class="badge" class:on={!!date} style:--i={i} title={b.hint}>
          <span class="ic"><Icon name={date ? b.icon : "lock"} size={22} /></span>
          <b>{b.label}</b>
          <small>{date ? `Obtenu le ${dateLabel(date)}` : b.hint}</small>
        </div>
      {/each}
    </div>
  </section>

  <section>
    <h3>Comment gagner des points</h3>
    <p class="muted">Envoyer un message (5), recevoir une réponse (3), faire modifier des fichiers (15), créer un projet (20) ou une tâche planifiée (20), ajouter un souvenir (10), garder un artefact (10), essayer un nouveau modèle (15), faire une sauvegarde (20). Chaque jour de série ajoute un bonus.</p>
    <div class="row"><button onclick={reset}>Réinitialiser la progression</button></div>
  </section>
{/if}

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  h4 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 26px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .check { display: flex; gap: 8px; align-items: center; font-size: 14px; }
  .hero { flex-direction: row; align-items: center; gap: 24px; border: 1px solid var(--border); background: var(--surface); border-radius: 16px; padding: 20px 24px; }
  .ring { position: relative; flex: none; width: 130px; height: 130px; }
  .ring svg { transform: rotate(-90deg); }
  .ring circle { fill: none; stroke-width: 10; }
  .track { stroke: var(--hover); }
  .bar { stroke: var(--accent); stroke-linecap: round; transition: stroke-dashoffset 1.2s cubic-bezier(0.2, 0.8, 0.2, 1); }
  .lv { position: absolute; inset: 0; display: grid; place-content: center; text-align: center; }
  .lv small { color: var(--muted); font-size: 12px; }
  .lv b { font-size: 38px; font-weight: 500; line-height: 1; font-family: var(--serif); }
  .info { display: grid; gap: 6px; min-width: 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 4px; }
  .chip { display: inline-flex; align-items: center; gap: 5px; background: var(--hover); border-radius: 999px; padding: 4px 12px; font-size: 13px; }
  .chip.hot { background: color-mix(in srgb, var(--accent) 20%, transparent); color: var(--accent); }
  .chip.hot :global(svg) { animation: flick 1.4s ease-in-out infinite; transform-origin: 50% 90%; }
  @keyframes flick { 50% { transform: scale(1.18) rotate(-4deg); } }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 10px; }
  .stats div { display: grid; gap: 2px; border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 12px 14px; animation: rise 0.5s calc(var(--i) * 60ms) cubic-bezier(0.2, 0.7, 0.2, 1) both; }
  .stats b { font-size: 24px; font-weight: 500; }
  .stats span { color: var(--muted); font-size: 12.5px; }
  .badges { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; }
  .badge { display: grid; justify-items: center; text-align: center; gap: 4px; border: 1px solid var(--border); background: var(--surface); border-radius: 14px; padding: 14px 10px; opacity: 0.55; animation: pop 0.45s calc(var(--i) * 40ms) cubic-bezier(0.2, 0.8, 0.2, 1) both; transition: transform 0.2s, opacity 0.2s; }
  .badge.on { opacity: 1; border-color: color-mix(in srgb, var(--accent) 50%, var(--border)); }
  .badge:hover { transform: translateY(-3px); opacity: 1; }
  .ic { display: grid; place-items: center; width: 46px; height: 46px; border-radius: 50%; background: var(--hover); color: var(--muted); }
  .badge.on .ic { background: linear-gradient(160deg, #de7650, #b4502e); color: #fff; }
  .badge b { font-size: 13.5px; font-weight: 600; }
  .badge small { color: var(--muted); font-size: 12px; line-height: 1.35; }
  .row { display: flex; }
  .row button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); }
  @media (max-width: 560px) { .hero { flex-direction: column; text-align: center; } .chips { justify-content: center; } }
</style>
