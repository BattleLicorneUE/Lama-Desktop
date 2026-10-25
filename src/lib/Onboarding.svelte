<script lang="ts">
  import type { ChatStore } from "$lib/chat.svelte";
  import Logo from "$lib/Logo.svelte";
  import {
    ADDRESSES, CONTEXTS, EMPTY, LANGUAGES, LENGTHS, LEVELS, TONES, USAGES, steps, toFacts, type Answers,
  } from "$lib/onboarding";

  let { chat, defaultName }: { chat: ChatStore; defaultName: string } = $props();

  // Réponses précédentes (si on refait la présentation), sinon le nom du profil : lecture voulue, une seule fois.
  // svelte-ignore state_referenced_locally
  let a = $state<Answers>({ ...EMPTY, ...chat.onboardingAnswers(), name: chat.onboardingAnswers().name || defaultName });
  let step = $state(0);
  let saving = $state(false);

  const list = $derived(steps(a));
  const current = $derived(list[Math.min(step, list.length - 1)]);
  const facts = $derived(toFacts(a));
  const last = $derived(step >= list.length - 1);

  function toggle(id: string) {
    a.usages = a.usages.includes(id) ? a.usages.filter((u) => u !== id) : [...a.usages, id];
  }

  /** Un choix unique : recliquer le désélectionne. */
  const pick = <K extends "context" | "level" | "length" | "tone" | "address" | "language">(key: K, value: string) => {
    a[key] = a[key] === value ? "" : value;
  };

  async function next() {
    if (!last) {
      step += 1;
      return;
    }
    saving = true;
    await chat.finishOnboarding($state.snapshot(a), facts);
    saving = false;
  }

  function focus(node: HTMLElement) {
    queueMicrotask(() => node.focus());
  }

  function onEnter(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && (e.target as HTMLElement).tagName !== "TEXTAREA") {
      e.preventDefault();
      next();
    }
  }
</script>

<div class="overlay top">
  <div class="dialog box" role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="ob-title" onkeydown={onEnter}>
    <div class="head">
      <Logo size={40} />
      <div>
        <h2 id="ob-title">Faisons connaissance</h2>
        <p class="sub">Quelques questions pour que Lama s'adapte à toi. Tout est facultatif et modifiable ensuite.</p>
      </div>
    </div>

    <div class="dots" aria-label="Étape {step + 1} sur {list.length}">
      {#each list as _, i}<span class:on={i <= step}></span>{/each}
    </div>

    <div class="body">
      {#if current === "name"}
        <label class="q" for="ob-name">Comment dois-je t'appeler ?</label>
        <input id="ob-name" use:focus bind:value={a.name} placeholder="Ton prénom ou un surnom" maxlength="40" />
      {:else if current === "usages"}
        <p class="q">Pour quoi vas-tu surtout utiliser Lama ? <span class="hint">(plusieurs choix possibles)</span></p>
        <div class="chips">
          {#each USAGES as u}
            <button type="button" class="chip" class:on={a.usages.includes(u.id)} aria-pressed={a.usages.includes(u.id)} onclick={() => toggle(u.id)}>{u.label}</button>
          {/each}
        </div>
      {:else if current === "context"}
        <p class="q">Dans quel cadre ?</p>
        <div class="chips">
          {#each CONTEXTS as c}
            <button type="button" class="chip" class:on={a.context === c} aria-pressed={a.context === c} onclick={() => pick("context", c)}>{c}</button>
          {/each}
        </div>
        <label class="q" for="ob-field">Ton métier ou ton domaine ? <span class="hint">(facultatif)</span></label>
        <input id="ob-field" bind:value={a.field} placeholder="Ex. développeur web, étudiante en droit, infirmier…" maxlength="120" />
      {:else if current === "tech"}
        <p class="q">Ton niveau en programmation ?</p>
        <div class="chips">
          {#each LEVELS as l}
            <button type="button" class="chip" class:on={a.level === l} aria-pressed={a.level === l} onclick={() => pick("level", l)}>{l}</button>
          {/each}
        </div>
        <label class="q" for="ob-tools">Langages et outils que tu utilises ? <span class="hint">(facultatif)</span></label>
        <input id="ob-tools" bind:value={a.tools} placeholder="Ex. Rust, TypeScript, Svelte, Unity…" maxlength="160" />
      {:else if current === "style"}
        <p class="q">Tes réponses idéales ?</p>
        <div class="chips">{#each LENGTHS as l}<button type="button" class="chip" class:on={a.length === l} aria-pressed={a.length === l} onclick={() => pick("length", l)}>{l}</button>{/each}</div>
        <p class="q">Le ton ?</p>
        <div class="chips">{#each TONES as t}<button type="button" class="chip" class:on={a.tone === t} aria-pressed={a.tone === t} onclick={() => pick("tone", t)}>{t}</button>{/each}</div>
        <div class="two">
          <div>
            <p class="q">Comment te parler ?</p>
            <div class="chips">{#each ADDRESSES as d}<button type="button" class="chip" class:on={a.address === d} aria-pressed={a.address === d} onclick={() => pick("address", d)}>{d}</button>{/each}</div>
          </div>
          <div>
            <p class="q">Langue des réponses ?</p>
            <div class="chips">{#each LANGUAGES as l}<button type="button" class="chip" class:on={a.language === l} aria-pressed={a.language === l} onclick={() => pick("language", l)}>{l}</button>{/each}</div>
          </div>
        </div>
      {:else if current === "note"}
        <label class="q" for="ob-note">Autre chose que Lama devrait savoir ? <span class="hint">(facultatif)</span></label>
        <textarea id="ob-note" use:focus bind:value={a.note} rows="5" maxlength="500" placeholder="Ex. « Je préfère les exemples concrets », « Mon projet principal est un jeu en Unity », « Évite le jargon »…"></textarea>
      {:else}
        <p class="q">Voilà ce que Lama retiendra :</p>
        {#if facts.length}
          <ul class="recap">{#each facts as f}<li>{f.replace(/^Profil — /, "")}</li>{/each}</ul>
        {:else}
          <p class="hint">Rien pour l'instant : tu peux revenir en arrière ou terminer sans rien enregistrer.</p>
        {/if}
        <p class="hint">Ces informations restent sur ce PC, dans ton profil. Tu peux les relire, les modifier ou les supprimer dans Réglages → Mémoire.</p>
      {/if}
    </div>

    <div class="foot">
      <button type="button" class="link" onclick={() => chat.skipOnboarding()}>Passer</button>
      <span class="sp"></span>
      {#if step > 0}<button type="button" onclick={() => (step -= 1)}>Retour</button>{/if}
      <button type="button" class="primary" onclick={next} disabled={saving}>{last ? "Enregistrer" : "Suivant"}</button>
    </div>
  </div>
</div>

<style>
  .box { max-width: 560px; display: flex; flex-direction: column; gap: 16px; padding: 24px; }
  .head { display: flex; gap: 14px; align-items: center; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 24px; }
  .sub { margin: 4px 0 0; color: var(--muted); font-size: 13.5px; line-height: 1.45; }
  .dots { display: flex; gap: 6px; }
  .dots span { flex: 1; height: 4px; border-radius: 2px; background: var(--hover); transition: background 0.2s; }
  .dots span.on { background: var(--accent); }
  .body { display: flex; flex-direction: column; gap: 12px; min-height: 190px; }
  .q { margin: 0; font-weight: 500; }
  .hint { color: var(--muted); font-weight: 400; font-size: 13px; }
  p.hint { margin: 0; line-height: 1.5; }
  input, textarea { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 9px 12px; outline: none; resize: vertical; line-height: 1.5; }
  input:focus, textarea:focus { border-color: var(--accent); }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 6px 14px; font-size: 14px; color: var(--fg); }
  .chip:hover { border-color: var(--accent); }
  .chip.on { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .two .q { margin-bottom: 8px; }
  @media (max-width: 560px) { .two { grid-template-columns: 1fr; } }
  .recap { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 6px; line-height: 1.45; }
  .foot { display: flex; align-items: center; gap: 8px; }
  .sp { flex: 1; }
  .foot button { border: 1px solid var(--border); background: var(--surface); border-radius: 9px; padding: 7px 18px; }
  .foot button:hover:not(:disabled) { border-color: var(--accent); }
  .foot .link { border: 0; background: none; color: var(--muted); padding: 7px 4px; }
  .foot .link:hover { color: var(--fg); }
  .foot .primary { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
</style>
