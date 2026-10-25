<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { askConfirm } from "$lib/dialogs.svelte";
  import Icon from "$lib/Icon.svelte";
  import Logo from "$lib/Logo.svelte";
  import { AVATAR_COLORS, initialOf, type Profile } from "$lib/profiles";

  let { profiles, error = "", opening = "", onopen, onchange }: {
    profiles: Profile[];
    error?: string;
    opening?: string;
    /** Ouvre un profil ; renvoie le message d'erreur éventuel (code PIN incorrect…), vide si tout va bien. */
    onopen: (p: Profile, pin?: string) => Promise<string>;
    onchange: () => void;
  } = $props();

  let manage = $state(false);
  type Editor = { id: string; name: string; color: string; locked: boolean; currentPin: string; newPin: string; removePin: boolean };
  /** `null` = fermé ; `id` vide = nouveau profil. */
  let editor = $state<Editor | null>(null);
  let problem = $state("");
  /** Saisie du code d'un profil verrouillé. */
  let unlock = $state<{ profile: Profile; pin: string; error: string } | null>(null);

  function click(p: Profile) {
    if (manage) return startEdit(p);
    if (p.locked) unlock = { profile: p, pin: "", error: "" };
    else onopen(p);
  }

  async function submitUnlock(e: Event) {
    e.preventDefault();
    if (!unlock) return;
    const err = await onopen(unlock.profile, unlock.pin);
    if (err) unlock = { ...unlock, pin: "", error: err };
    else unlock = null;
  }

  /** Premier lancement : le premier profil se crée ici, puis s'ouvre tout de suite (le tutoriel suit). */
  let first = $state({ name: "", color: AVATAR_COLORS[0], busy: false, problem: "" });
  async function createFirst(e: Event) {
    e.preventDefault();
    first.busy = true;
    first.problem = "";
    try {
      const p = await invoke<Profile>("profile_create", { name: first.name, color: first.color });
      const err = await onopen(p);
      if (err) first.problem = err;
    } catch (err) {
      first.problem = String(err);
    } finally {
      first.busy = false;
    }
  }

  function startCreate() {
    problem = "";
    editor = { id: "", name: "", color: AVATAR_COLORS[profiles.length % AVATAR_COLORS.length], locked: false, currentPin: "", newPin: "", removePin: false };
  }

  function startEdit(p: Profile) {
    problem = "";
    editor = { id: p.id, name: p.name, color: p.color, locked: p.locked, currentPin: "", newPin: "", removePin: false };
  }

  async function save(e: Event) {
    e.preventDefault();
    if (!editor) return;
    const { id, name, color, currentPin, newPin, removePin } = editor;
    try {
      if (id) {
        await invoke("profile_update", { id, name, color, pin: currentPin || null });
        if (removePin) await invoke("profile_set_pin", { id, newPin: null, currentPin: currentPin || null });
        else if (newPin) await invoke("profile_set_pin", { id, newPin, currentPin: currentPin || null });
      } else {
        const p = await invoke<Profile>("profile_create", { name, color });
        if (newPin) await invoke("profile_set_pin", { id: p.id, newPin, currentPin: null });
      }
      editor = null;
      onchange();
    } catch (err) {
      problem = String(err);
    }
  }

  async function remove() {
    if (!editor?.id) return;
    const { id, name, currentPin } = editor;
    if (!(await askConfirm(`Supprimer le profil « ${name} » ? Ses conversations, projets, mémoire, skills et réglages seront mis à la corbeille.`, "Supprimer le profil"))) return;
    try {
      await invoke("profile_delete", { id, pin: currentPin || null });
      editor = null;
      onchange();
    } catch (err) {
      problem = String(err);
    }
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

{#if profiles.length === 0}
<main class="stage">
  <Logo size={72} animated />
  <h1 class="rise" style="--d: 0.25s">Bienvenue dans Lama</h1>
  <p class="note rise" style="--d: 0.4s">Commençons par créer ton profil. Il garde tes conversations, ta mémoire et tes réglages sur ce PC.</p>
  <form class="first rise" style="--d: 0.55s" onsubmit={createFirst}>
    <span class="avatar big" style:background={first.color}>{initialOf(first.name)}</span>
    <input use:focusSelect bind:value={first.name} placeholder="Ton prénom ou un surnom" maxlength="24" aria-label="Ton prénom ou un surnom" />
    <div class="colors" role="radiogroup" aria-label="Couleur">
      {#each AVATAR_COLORS as c}
        <button type="button" class="swatch" class:on={first.color === c} style:background={c} role="radio" aria-checked={first.color === c} aria-label={c} onclick={() => (first.color = c)}></button>
      {/each}
    </div>
    {#if first.problem || error}<p class="err" role="alert">{first.problem || error}</p>{/if}
    <button class="cta" disabled={!first.name.trim() || first.busy || !!opening}>{first.busy || opening ? "Création…" : "Continuer"}</button>
  </form>
</main>
{:else}
<main class="stage">
  <Logo size={72} animated />
  <h1 class="rise" style="--d: 0.25s">Qui utilise Lama Desktop ?</h1>
  {#if error}<p class="err" role="alert">{error}</p>{/if}

  <div class="grid">
    {#each profiles as p, i (p.id)}
      <div class="tile rise" style:--d="{0.4 + i * 0.09}s" class:busy={opening === p.id}>
        <button class="avatar" style:background={p.color} disabled={!!opening} onclick={() => click(p)}
          aria-label={manage ? `Modifier le profil ${p.name}` : `Ouvrir le profil ${p.name}${p.locked ? " (verrouillé)" : ""}`}>
          {initialOf(p.name)}
          {#if manage}<span class="veil"><Icon name="pencil" size={30} /></span>{/if}
          {#if opening === p.id}<span class="veil"><span class="spin"></span></span>{/if}
          {#if p.locked && !manage}<span class="lock" aria-hidden="true"><Icon name="shield" size={16} /></span>{/if}
        </button>
        <span class="name">{opening === p.id ? "Ouverture…" : p.name}</span>
      </div>
    {/each}
    <div class="tile rise" style:--d="{0.4 + profiles.length * 0.09}s">
      <button class="avatar add" onclick={startCreate} aria-label="Ajouter un profil"><Icon name="plus" size={38} /></button>
      <span class="name">Ajouter un profil</span>
    </div>
  </div>

  <button class="manage rise" style="--d: 0.9s" onclick={() => (manage = !manage)}>{manage ? "Terminé" : "Gérer les profils"}</button>
  <p class="note rise" style="--d: 1s">Chaque profil a ses propres conversations, projets, mémoire, skills, agents, connecteurs et réglages.</p>
</main>
{/if}

{#if unlock}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onmousedown={(e) => e.target === e.currentTarget && (unlock = null)}>
    <div class="dialog small" role="dialog" aria-modal="true" aria-label="Code PIN">
      <form class="form" onsubmit={submitUnlock}>
        <div class="preview"><span class="avatar big" style:background={unlock.profile.color}>{initialOf(unlock.profile.name)}</span></div>
        <h2>{unlock.profile.name}</h2>
        <input use:focusSelect bind:value={unlock.pin} type="password" inputmode="numeric" autocomplete="off" placeholder="Code PIN" aria-label="Code PIN" maxlength="12" />
        {#if unlock.error}<p class="err" role="alert">{unlock.error}</p>{/if}
        <div class="row">
          <span class="sp"></span>
          <button type="button" onclick={() => (unlock = null)}>Annuler</button>
          <button class="primary" disabled={!unlock.pin.trim() || !!opening}>Ouvrir</button>
        </div>
      </form>
    </div>
  </div>
{/if}

{#if editor}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onmousedown={(e) => e.target === e.currentTarget && (editor = null)}>
    <div class="dialog small" role="dialog" aria-modal="true" aria-label={editor.id ? "Modifier le profil" : "Nouveau profil"}>
      <form class="form" onsubmit={save}>
        <h2>{editor.id ? "Modifier le profil" : "Nouveau profil"}</h2>
        <div class="preview"><span class="avatar big" style:background={editor.color}>{initialOf(editor.name)}</span></div>
        <input use:focusSelect bind:value={editor.name} placeholder="Nom du profil" maxlength="24" aria-label="Nom du profil" />
        <div class="colors" role="radiogroup" aria-label="Couleur">
          {#each AVATAR_COLORS as c}
            <button type="button" class="swatch" class:on={editor.color === c} style:background={c} role="radio" aria-checked={editor.color === c} aria-label={c} onclick={() => editor && (editor.color = c)}></button>
          {/each}
        </div>

        <fieldset class="pin">
          <legend>Code PIN (facultatif)</legend>
          {#if editor.locked}
            <input bind:value={editor.currentPin} type="password" inputmode="numeric" autocomplete="off" placeholder="Code actuel (obligatoire)" aria-label="Code actuel" maxlength="12" />
          {/if}
          {#if !editor.removePin}
            <input bind:value={editor.newPin} type="password" inputmode="numeric" autocomplete="off" placeholder={editor.locked ? "Nouveau code (4 à 12 chiffres)" : "Choisir un code (4 à 12 chiffres)"} aria-label="Nouveau code" maxlength="12" />
          {/if}
          {#if editor.locked}
            <label class="check"><input type="checkbox" bind:checked={editor.removePin} /> Retirer le code</label>
          {/if}
          <p class="hint">Le code bloque l'ouverture du profil depuis l'application. Il ne chiffre pas les fichiers : quelqu'un qui ouvre le dossier de données de l'application sur ce PC peut les lire.</p>
        </fieldset>

        {#if problem}<p class="err">{problem}</p>{/if}
        <div class="row">
          {#if editor.id && profiles.length > 1}<button type="button" class="danger" onclick={remove}>Supprimer</button>{/if}
          <span class="sp"></span>
          <button type="button" onclick={() => (editor = null)}>Annuler</button>
          <button class="primary" disabled={!editor.name.trim() || (editor.locked && !editor.currentPin.trim())}>Enregistrer</button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style>
  .stage { min-height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 26px; padding: 30px 20px; }
  h1 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 34px; text-align: center; }
  .grid { display: flex; flex-wrap: wrap; justify-content: center; gap: 28px; max-width: 760px; }
  .tile { display: flex; flex-direction: column; align-items: center; gap: 10px; width: 120px; }
  .avatar { position: relative; display: grid; place-items: center; width: 108px; height: 108px; border: 3px solid transparent; border-radius: 18px; color: #fff; font-size: 46px; font-weight: 600; overflow: hidden; }
  .tile:hover .avatar, .avatar:focus-visible { border-color: var(--fg); transform: scale(1.04); }
  .avatar { transition: transform 0.12s, border-color 0.12s; }
  .avatar.add { background: var(--surface); color: var(--muted); border: 2px dashed var(--border); }
  .avatar.big { width: 76px; height: 76px; font-size: 32px; border-radius: 14px; cursor: default; }
  .veil { position: absolute; inset: 0; display: grid; place-items: center; background: rgb(0 0 0 / 0.5); }
  .lock { position: absolute; right: 6px; bottom: 6px; display: grid; place-items: center; width: 24px; height: 24px; border-radius: 7px; background: rgb(0 0 0 / 0.45); }
  .tile.busy .name { color: var(--fg); }
  .avatar:disabled { cursor: progress; }
  .tile:not(.busy) .avatar:disabled { opacity: 0.5; }
  .spin { width: 34px; height: 34px; border-radius: 50%; border: 4px solid rgb(255 255 255 / 0.35); border-top-color: #fff; animation: turn 0.8s linear infinite; }
  @keyframes turn { to { transform: rotate(360deg); } }
  .name { color: var(--muted); text-align: center; overflow-wrap: anywhere; }
  .tile:hover .name { color: var(--fg); }
  .manage { border: 1px solid var(--border); background: none; border-radius: 10px; padding: 8px 20px; color: var(--muted); letter-spacing: 0.04em; }
  .manage:hover { color: var(--fg); border-color: var(--fg); }
  .note { margin: 0; color: var(--muted); font-size: 13px; text-align: center; max-width: 440px; line-height: 1.5; }
  .err { color: var(--del); margin: 0; font-size: 14px; }
  .first { display: flex; flex-direction: column; align-items: center; gap: 16px; width: min(340px, 100%); }
  .first input { width: 100%; font-size: 16px; text-align: center; }
  .cta { width: 100%; border: 1px solid var(--accent); background: var(--accent); color: var(--accent-fg); border-radius: 10px; padding: 10px; font-size: 15px; }
  .cta:disabled { opacity: 0.5; }
  .dialog.small { max-width: 400px; }
  .form { display: flex; flex-direction: column; gap: 14px; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 22px; text-align: center; }
  .preview { display: flex; justify-content: center; }
  input:not([type="checkbox"]) { border: 1px solid var(--border); background: var(--surface); border-radius: 10px; padding: 9px 12px; outline: none; }
  input:focus { border-color: var(--accent); }
  .colors { display: flex; gap: 8px; justify-content: center; flex-wrap: wrap; }
  .swatch { width: 28px; height: 28px; border-radius: 50%; border: 3px solid transparent; padding: 0; }
  .swatch.on { border-color: var(--fg); }
  .pin { border: 1px solid var(--border); border-radius: 12px; padding: 10px 12px 12px; display: flex; flex-direction: column; gap: 8px; margin: 0; }
  .pin legend { padding: 0 6px; color: var(--muted); font-size: 13px; }
  .check { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--muted); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; line-height: 1.45; }
  .row { display: flex; gap: 8px; align-items: center; }
  .sp { flex: 1; }
  .row button { border: 1px solid var(--border); background: var(--surface); border-radius: 9px; padding: 7px 16px; }
  .row button:hover:not(:disabled) { border-color: var(--accent); }
  .row button:disabled { opacity: 0.45; }
  .primary { background: var(--accent) !important; color: var(--accent-fg); border-color: var(--accent) !important; }
  .danger { color: var(--del); }
</style>
