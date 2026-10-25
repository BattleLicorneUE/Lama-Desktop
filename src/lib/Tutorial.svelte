<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { preferredModel, type ChatStore } from "$lib/chat.svelte";
  import Logo from "$lib/Logo.svelte";
  import { ADDRESSES, EMPTY, LENGTHS, LEVELS, TONES, USAGES, toFacts, type Answers } from "$lib/onboarding";

  let { chat, defaultName }: { chat: ChatStore; defaultName: string } = $props();

  // Réponses précédentes (si on rouvre le tutoriel), sinon le nom du profil : lecture voulue, une seule fois.
  // svelte-ignore state_referenced_locally
  // Le prénom est celui du profil, choisi juste avant : inutile de le redemander.
  let a = $state<Answers>({ ...EMPTY, ...chat.onboardingAnswers(), name: defaultName });
  let step = $state(0);
  let key = $state("");
  let connecting = $state(false);
  let saving = $state(false);

  // Une seule question par écran ; le niveau en programmation n'est demandé qu'à ceux qui programment.
  const list = $derived(["welcome", "usages", ...(a.usages.includes("code") ? ["level"] : []), "style", "tour", "key"]);
  const current = $derived(list[Math.min(step, list.length - 1)]);
  const last = $derived(step >= list.length - 1);
  const connected = $derived(chat.cloudStatus === "ok");

  const toggle = (id: string) => (a.usages = a.usages.includes(id) ? a.usages.filter((u) => u !== id) : [...a.usages, id]);
  /** Un choix unique : recliquer le désélectionne. */
  const pick = <K extends "level" | "length" | "tone" | "address">(k: K, v: string) => (a[k] = a[k] === v ? "" : v);

  async function connect() {
    connecting = true;
    await chat.connectCloud(key);
    connecting = false;
    if (chat.cloudStatus === "ok") {
      key = "";
      // Le modèle par défaut (beam) a servi au test de la clé, donc il est accessible : on s'en sert tant qu'aucun autre n'est choisi.
      if (!chat.model) chat.setModel(preferredModel(chat.models)?.name ?? "");
    }
  }

  async function finish() {
    saving = true;
    await chat.finishOnboarding($state.snapshot(a), toFacts(a));
    chat.finishTutorial();
  }

  function skip() {
    chat.skipOnboarding();
    chat.finishTutorial(true);
  }

  function next() {
    if (last) finish();
    else step += 1;
  }

  function onEnter(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (e.key === "Enter" && !e.shiftKey && t.tagName !== "BUTTON" && current !== "key") {
      e.preventDefault();
      next();
    }
  }

  const focus = (node: HTMLElement) => queueMicrotask(() => node.focus());

  const TOUR = [
    { t: "Chat", d: "Pose une question, fais écrire, résumer, traduire, chercher sur le web." },
    { t: "Code", d: "Choisis un dossier de ton PC : Lama lit, modifie et vérifie tes fichiers." },
    { t: "Tout reste chez toi", d: "Tes conversations sont sur ce PC. Tape / pour voir les commandes." },
  ];
</script>

<div class="overlay top">
  <div class="dialog box" role="dialog" aria-modal="true" aria-label="Bienvenue" tabindex="-1" onkeydown={onEnter}>
    <div class="dots" aria-label="Étape {step + 1} sur {list.length}">
      {#each list as _, i}<span class:on={i <= step}></span>{/each}
    </div>

    {#key current}
    <div class="body">
      {#if current === "welcome"}
        <div class="center">
          <Logo size={64} animated />
          <h2>Bienvenue {defaultName}</h2>
          <p>Ton assistant IA sur PC. Quelques questions pour qu'il s'adapte à toi, puis on le branche. Moins d'une minute.</p>
        </div>
      {:else if current === "usages"}
        <h2>Tu vas surtout l'utiliser pour…</h2>
        <p class="hint">Choisis ce qui te parle, plusieurs réponses possibles.</p>
        <div class="chips">
          {#each USAGES as u, i}
            <button type="button" class="chip" style:--i={i} class:on={a.usages.includes(u.id)} aria-pressed={a.usages.includes(u.id)} onclick={() => toggle(u.id)}>{u.label}</button>
          {/each}
        </div>
      {:else if current === "level"}
        <h2>Ton niveau en programmation ?</h2>
        <div class="chips">
          {#each LEVELS as l}<button type="button" class="chip" class:on={a.level === l} aria-pressed={a.level === l} onclick={() => pick("level", l)}>{l}</button>{/each}
        </div>
      {:else if current === "style"}
        <h2>Tes réponses idéales ?</h2>
        <p class="q">Longueur</p>
        <div class="chips">{#each LENGTHS as l}<button type="button" class="chip" class:on={a.length === l} aria-pressed={a.length === l} onclick={() => pick("length", l)}>{l}</button>{/each}</div>
        <p class="q">Ton</p>
        <div class="chips">{#each TONES as t}<button type="button" class="chip" class:on={a.tone === t} aria-pressed={a.tone === t} onclick={() => pick("tone", t)}>{t}</button>{/each}</div>
        <p class="q">Il te parle en</p>
        <div class="chips">{#each ADDRESSES as d}<button type="button" class="chip" class:on={a.address === d} aria-pressed={a.address === d} onclick={() => pick("address", d)}>{d}</button>{/each}</div>
      {:else if current === "tour"}
        <h2>Ce que tu peux faire</h2>
        <ul class="tour">
          {#each TOUR as f, i}<li style:--i={i}><b>{f.t}</b><span>{f.d}</span></li>{/each}
        </ul>
      {:else}
        <h2>Dernière étape : ta clé API</h2>
        <p>Elle permet à Lama d'utiliser les modèles. Elle reste sur ce PC, dans le coffre de Windows.</p>
        {#if connected}
          <p class="ok" role="status">Connecté : tout est prêt.</p>
          <p class="hint">Tu pourras changer de modèle sous la zone de saisie. Réglages → Modèles contient un guide par usage et par prix.</p>
        {:else}
          <form class="row" onsubmit={(e) => { e.preventDefault(); if (key.trim()) connect(); }}>
            <input use:focus type="password" bind:value={key} placeholder="Colle ta clé API ici" autocomplete="off" spellcheck="false" aria-label="Clé API" />
            <button class="primary" disabled={!key.trim() || connecting}>{connecting ? "Test…" : "Connecter"}</button>
          </form>
          {#if chat.cloudStatus === "error"}<p class="err" role="alert">{chat.cloudError}</p>{/if}
          <button class="link" onclick={() => openUrl("https://ollama.com/settings/keys").catch(() => {})}>Où la trouver ?</button>
        {/if}
      {/if}
    </div>
    {/key}

    <div class="foot">
      <button class="link" onclick={skip}>Passer</button>
      <span class="sp"></span>
      {#if step > 0}<button onclick={() => (step -= 1)}>Retour</button>{/if}
      <button class="primary" onclick={next} disabled={saving}>{last ? (connected ? "Commencer" : "Plus tard") : current === "welcome" ? "C'est parti" : "Suivant"}</button>
    </div>
  </div>
</div>

<style>
  .box { max-width: 520px; display: flex; flex-direction: column; gap: 18px; padding: 26px; }
  .dots { display: flex; gap: 6px; }
  .dots span { flex: 1; height: 4px; border-radius: 2px; background: var(--hover); transition: background 0.35s, transform 0.35s; transform-origin: left; }
  .dots span.on { transform: scaleY(1.4); }
  .dots span.on { background: var(--accent); }
  .body > :global(*) { animation: rise 0.4s cubic-bezier(0.2, 0.7, 0.2, 1) both; animation-delay: calc(var(--i, 0) * 60ms); }
  .chip { animation: pop 0.35s cubic-bezier(0.2, 0.8, 0.2, 1) both; animation-delay: calc(var(--i, 0) * 35ms); }
  .tour li { animation: rise 0.45s cubic-bezier(0.2, 0.7, 0.2, 1) both; animation-delay: calc(var(--i, 0) * 90ms); }
  .body { display: flex; flex-direction: column; gap: 14px; min-height: 250px; }
  .center { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 14px; padding-top: 14px; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 26px; line-height: 1.2; }
  p { margin: 0; line-height: 1.55; }
  .hint { color: var(--muted); font-size: 13.5px; }
  .q { font-weight: 500; font-size: 14px; margin-top: 2px; }
  input { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 10px 14px; outline: none; color: var(--fg); font-size: 15px; min-width: 0; }
  .row input { flex: 1; }
  input:focus { border-color: var(--accent); }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 7px 16px; font-size: 14.5px; color: var(--fg); }
  .chip:hover { border-color: var(--accent); }
  .chip.on { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
  .tour { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
  .tour li { display: grid; gap: 2px; border: 1px solid var(--border); background: var(--surface); border-radius: 12px; padding: 12px 14px; }
  .tour b { font-size: 15.5px; font-weight: 600; }
  .tour span { color: var(--muted); font-size: 14px; line-height: 1.45; }
  .row { display: flex; gap: 8px; }
  .ok { color: var(--add); font-weight: 500; }
  .err { color: var(--del); font-size: 13px; }
  .foot { display: flex; align-items: center; gap: 8px; }
  .sp { flex: 1; }
  .foot button, .row button { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 8px 20px; color: var(--fg); }
  .foot button:hover:not(:disabled) { border-color: var(--accent); }
  .link { border: 0 !important; background: none !important; color: var(--muted); padding: 7px 4px !important; font: inherit; align-self: flex-start; }
  .body .link { color: var(--accent); text-decoration: underline; padding: 0 !important; font-size: 13.5px; }
  .foot .link:hover { color: var(--fg); }
  .primary { background: var(--accent) !important; color: var(--accent-fg) !important; border-color: var(--accent) !important; }
  .primary:disabled { opacity: 0.55; }
</style>
