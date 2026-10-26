<script lang="ts">
  import type { ChatStore } from "$lib/chat.svelte";
  import {
    ADDRESSES, CONTEXTS, EMPTY, LANGUAGES, LENGTHS, LEVELS, TONES, USAGES, toFacts, type Answers,
  } from "$lib/onboarding";

  let { chat, defaultName }: { chat: ChatStore; defaultName: string } = $props();

  // Réponses enregistrées la dernière fois (ou le nom du profil au départ) : lecture voulue, une seule fois.
  // svelte-ignore state_referenced_locally
  let a = $state<Answers>({ ...EMPTY, ...chat.onboardingAnswers(), name: chat.onboardingAnswers().name || defaultName });
  let saved = $state(JSON.stringify($state.snapshot(a)));
  let busy = $state(false);

  const dirty = $derived(JSON.stringify($state.snapshot(a)) !== saved);
  const facts = $derived(toFacts(a));

  function toggle(id: string) {
    a.usages = a.usages.includes(id) ? a.usages.filter((u) => u !== id) : [...a.usages, id];
  }

  /** Un choix unique : recliquer le désélectionne. */
  const pick = (key: "context" | "level" | "length" | "tone" | "address" | "language", value: string) => {
    a[key] = a[key] === value ? "" : value;
  };

  async function save() {
    busy = true;
    await chat.finishOnboarding($state.snapshot(a), facts);
    saved = JSON.stringify($state.snapshot(a));
    busy = false;
  }
</script>

<div class="panel">
  <p class="muted">Ces réponses sont gardées dans la mémoire de ce profil et envoyées au modèle dans chaque conversation. Change-les quand ta situation évolue.
  Rien ne quitte ce PC, à part les messages envoyés à Ollama Cloud.</p>
  <p class="muted">
    Pour remplir des formulaires (adresse, téléphone, mots de passe), utilise plutôt le <b>Coffre</b> (Réglages → Coffre) : ces informations y sont chiffrées et jamais envoyées au modèle.
  </p>

  <section>
    <h3>Toi</h3>
    <label class="field">Comment t'appeler ?
      <input bind:value={a.name} maxlength="40" placeholder="Ton prénom ou un surnom" />
    </label>
    <div class="field">Usages principaux
      <div class="chips">
        {#each USAGES as u}
          <button type="button" class="chip" class:on={a.usages.includes(u.id)} aria-pressed={a.usages.includes(u.id)} onclick={() => toggle(u.id)}>{u.label}</button>
        {/each}
      </div>
    </div>
    <div class="field">Cadre
      <div class="chips">
        {#each CONTEXTS as c}
          <button type="button" class="chip" class:on={a.context === c} aria-pressed={a.context === c} onclick={() => pick("context", c)}>{c}</button>
        {/each}
      </div>
    </div>
    <label class="field">Métier ou domaine
      <input bind:value={a.field} maxlength="120" placeholder="Ex. développeur web, étudiante en droit, infirmier…" />
    </label>
  </section>

  <section>
    <h3>Programmation</h3>
    <div class="field">Niveau
      <div class="chips">
        {#each LEVELS as l}
          <button type="button" class="chip" class:on={a.level === l} aria-pressed={a.level === l} onclick={() => pick("level", l)}>{l}</button>
        {/each}
      </div>
    </div>
    <label class="field">Langages et outils
      <input bind:value={a.tools} maxlength="160" placeholder="Ex. Rust, TypeScript, Svelte, Unity…" />
    </label>
  </section>

  <section>
    <h3>Style des réponses</h3>
    <div class="field">Longueur
      <div class="chips">{#each LENGTHS as l}<button type="button" class="chip" class:on={a.length === l} aria-pressed={a.length === l} onclick={() => pick("length", l)}>{l}</button>{/each}</div>
    </div>
    <div class="field">Ton
      <div class="chips">{#each TONES as t}<button type="button" class="chip" class:on={a.tone === t} aria-pressed={a.tone === t} onclick={() => pick("tone", t)}>{t}</button>{/each}</div>
    </div>
    <div class="field">Manière de te parler
      <div class="chips">{#each ADDRESSES as d}<button type="button" class="chip" class:on={a.address === d} aria-pressed={a.address === d} onclick={() => pick("address", d)}>{d}</button>{/each}</div>
    </div>
    <div class="field">Langue des réponses
      <div class="chips">{#each LANGUAGES as l}<button type="button" class="chip" class:on={a.language === l} aria-pressed={a.language === l} onclick={() => pick("language", l)}>{l}</button>{/each}</div>
    </div>
  </section>

  <section>
    <h3>À savoir d'autre</h3>
    <textarea bind:value={a.note} rows="4" maxlength="500" aria-label="Autre chose que Lama devrait savoir" placeholder="Ex. « Je préfère les exemples concrets », « Mon projet principal est un jeu en Unity », « Évite le jargon »…"></textarea>
  </section>

  <section>
    <h3>Aperçu de ce qui est retenu</h3>
    {#if facts.length}
      <ul class="recap">{#each facts as f}<li>{f.replace(/^Profil — /, "")}</li>{/each}</ul>
    {:else}
      <p class="muted">Rien pour l'instant.</p>
    {/if}
  </section>

  <div class="bar">
    <button type="button" onclick={() => { chat.onboardingOpen = true; }} title="Reprendre les questions une par une">Refaire avec l'assistant</button>
    <span class="sp"></span>
    {#if !dirty && chat.onboarded}<span class="muted ok">Enregistré</span>{/if}
    <button type="button" class="primary" onclick={save} disabled={busy || (!dirty && chat.onboarded)}>{busy ? "Enregistrement…" : "Enregistrer"}</button>
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; gap: 16px; }
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0; font-size: 14px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.5; }
  .field { display: flex; flex-direction: column; gap: 6px; font-size: 14px; }
  input, textarea { border: 1px solid var(--border); background: var(--surface); border-radius: 9px; padding: 8px 12px; outline: none; resize: vertical; line-height: 1.5; }
  input:focus, textarea:focus { border-color: var(--accent); }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 5px 13px; font-size: 13.5px; color: var(--fg); }
  .chip:hover { border-color: var(--accent); }
  .chip.on { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
  .recap { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 5px; font-size: 13.5px; line-height: 1.45; }
  .bar { display: flex; align-items: center; gap: 10px; position: sticky; bottom: -20px; background: var(--bg); padding: 10px 0 4px; border-top: 1px solid var(--border); }
  .sp { flex: 1; }
  .ok { color: var(--add); }
  .bar button { border: 1px solid var(--border); background: var(--surface); border-radius: 9px; padding: 7px 16px; }
  .bar button:hover:not(:disabled) { border-color: var(--accent); }
  .bar button:disabled { opacity: 0.5; }
  .bar .primary { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
</style>
