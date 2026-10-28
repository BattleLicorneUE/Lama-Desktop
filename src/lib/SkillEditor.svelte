<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { ChatStore } from "$lib/chat.svelte";
  import { answerConfirm, askConfirm, confirmState } from "$lib/dialogs.svelte";
  import { SKILL_TEMPLATES, SKILL_WRITER_PROMPT, parseSkillReply, slugify, type SkillDraft } from "$lib/skill-templates";

  let { chat, editing, onclose, onsaved }: { chat: ChatStore; editing: string | null; onclose: () => void; onsaved: (message: string) => void } = $props();

  type Loaded = { draft: SkillDraft; scope: "app" | "project"; dir: string };

  const MAX_DESC = 500;
  const MAX_BODY = 30000;
  const hasProject = $derived(chat.mode === "code" && !!chat.workspace);
  const workspace = $derived(chat.mode === "code" && chat.workspace ? chat.workspace : null);

  let name = $state("");
  let description = $state("");
  let body = $state("");
  let argHint = $state("");
  let modelInvocable = $state(true);
  let scope = $state<"app" | "project">("app");
  let idea = $state("");
  let drafting = $state(false);
  let saving = $state(false);
  let error = $state("");
  // svelte-ignore state_referenced_locally
  let loading = $state(!!editing);
  let showOptions = $state(false);
  let template = $state("");
  let initial = $state("");

  const snapshot = () => JSON.stringify([name, description, body, argHint, modelInvocable]);
  const dirty = $derived(snapshot() !== initial);

  onMount(async () => {
    if (editing) {
      try {
        const r = await invoke<Loaded>("skill_get", { workspace, name: editing });
        name = r.draft.name;
        description = r.draft.description;
        body = r.draft.body;
        argHint = r.draft.argument_hint;
        modelInvocable = r.draft.model_invocable;
        scope = r.scope;
        showOptions = !r.draft.model_invocable || !!r.draft.argument_hint;
      } catch (e) {
        error = String(e);
      }
      loading = false;
    }
    initial = snapshot();
  });

  const slug = $derived(slugify(name));
  const taken = $derived(!!slug && chat.skills.some((s) => s.name.toLowerCase() === slug && s.name.toLowerCase() !== (editing ?? "").toLowerCase()));
  const descLen = $derived(description.trim().length);
  const problems = $derived.by(() => {
    const p: string[] = [];
    if (!slug) p.push("Donne un nom au skill.");
    else if (taken) p.push(`Un skill « ${slug} » existe déjà.`);
    if (descLen < 8) p.push("Explique quand l'utiliser (une phrase suffit).");
    else if (descLen > MAX_DESC) p.push("La description est trop longue.");
    if (body.trim().length < 10) p.push("Écris les instructions.");
    else if (body.length > MAX_BODY) p.push("Les instructions sont trop longues.");
    return p;
  });
  // Un bon skill dit QUAND il sert : on le rappelle sans bloquer.
  const descTip = $derived(descLen >= 8 && !/quand|lorsque|si (on|l'utilisateur|tu)|demande/i.test(description) ? "Astuce : dis dans quels cas Lama doit s'en servir (« Utilise ce skill quand… »)." : "");

  function applyTemplate(id: string) {
    const t = SKILL_TEMPLATES.find((x) => x.id === id);
    if (!t) return;
    template = id;
    name = t.draft.name;
    description = t.draft.description;
    body = t.draft.body;
    argHint = t.draft.argument_hint;
    showOptions = !!t.draft.argument_hint;
  }

  async function pickTemplate(id: string) {
    if (dirty && (name || description || body) && !(await askConfirm("Remplacer ce que tu as déjà écrit par ce modèle ?", "Remplacer"))) return;
    applyTemplate(id);
  }

  async function writeWithLama() {
    error = "";
    if (!idea.trim()) return;
    if (!chat.model) {
      error = "Choisis d'abord un modèle dans la conversation : c'est lui qui rédige le skill.";
      return;
    }
    if ((description.trim() || body.trim()) && !(await askConfirm("Lama va remplacer la description et les instructions que tu as écrites. Continuer ?", "Remplacer"))) return;
    drafting = true;
    try {
      const out = await invoke<string>("chat_once", {
        model: chat.model,
        cloudKey: chat.webKey,
        options: { temperature: 0.4 },
        messages: [
          { role: "system", content: SKILL_WRITER_PROMPT },
          { role: "user", content: idea.trim() },
        ],
      });
      const d = parseSkillReply(out);
      if (!d) {
        error = "Lama n'a pas réussi à rédiger un skill exploitable. Reformule ton idée ou réessaie.";
        return;
      }
      if (!name.trim() && d.name) name = d.name;
      description = (d.description ?? "").trim();
      body = (d.body ?? "").trim();
      argHint = (d.argument_hint ?? "").trim();
      showOptions = !!argHint;
    } catch (e) {
      error = `La rédaction a échoué : ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      drafting = false;
    }
  }

  async function save() {
    error = "";
    if (problems.length) {
      error = problems[0];
      return;
    }
    saving = true;
    try {
      await invoke("skill_save", {
        workspace,
        draft: { name, description, body, argument_hint: argHint, model_invocable: modelInvocable },
        scope,
        original: editing,
      });
      await chat.loadSkills();
      onsaved(editing ? `Skill /${slug} enregistré.` : `Skill /${slug} créé. Tape /${slug}${argHint ? " " + argHint : ""} dans une conversation pour l'utiliser.`);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function cancel() {
    if (dirty && !(await askConfirm("Abandonner les changements de ce skill ?", "Abandonner"))) return;
    onclose();
  }

  function key(e: KeyboardEvent) {
    // Tant que l'éditeur est ouvert, Échap ne ferme que lui (pas les Réglages derrière) ; la confirmation d'abandon garde la main sur la touche.
    if (e.key === "Escape") {
      e.stopPropagation();
      e.preventDefault();
      // Une confirmation est ouverte : Échap la refuse, sans fermer l'éditeur.
      if (confirmState.current) answerConfirm(false);
      else void cancel();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void save();
    }
  }
</script>

<svelte:window onkeydowncapture={key} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay top" onmousedown={(e) => e.target === e.currentTarget && cancel()}>
  <div class="dialog editor" role="dialog" aria-modal="true" aria-label={editing ? `Modifier le skill ${editing}` : "Nouveau skill"}>
    <h3>{editing ? `Modifier /${editing}` : "Nouveau skill"}</h3>
    <p class="muted">Un skill, c'est une procédure que Lama suit pour une tâche précise. Tu l'écris une fois, il le retrouve tout seul ou quand tu tapes <code>/{slug || "nom"}</code>.</p>

    {#if loading}
      <p class="muted">Chargement…</p>
    {:else}
      {#if !editing}
        <div class="start">
          <div class="label">Pour commencer</div>
          <div class="chips" role="group" aria-label="Modèles de départ">
            {#each SKILL_TEMPLATES as t (t.id)}
              <button class="chip" class:on={template === t.id} onclick={() => pickTemplate(t.id)}>{t.label}</button>
            {/each}
          </div>
          <form class="idea" onsubmit={(e) => { e.preventDefault(); void writeWithLama(); }}>
            <input bind:value={idea} placeholder="…ou décris ton idée : « répondre aux mails de mes clients, poliment et en deux phrases »" disabled={drafting} aria-label="Ton idée de skill" />
            <button class="primary" disabled={!idea.trim() || drafting}>{drafting ? "Rédaction…" : "Rédiger avec Lama"}</button>
          </form>
        </div>
      {/if}

      <label class="field">
        <span class="label">Nom</span>
        <input bind:value={name} placeholder="ex. relire-un-texte" spellcheck="false" aria-invalid={taken} />
        <span class="hint" class:bad={taken}>{taken ? `« ${slug} » existe déjà.` : slug ? `Commande : /${slug}` : "Des lettres, des chiffres et des tirets."}</span>
      </label>

      <label class="field">
        <span class="label">Quand l'utiliser ?</span>
        <textarea rows="2" bind:value={description} placeholder="Utilise ce skill quand on demande de relire ou de corriger un texte."></textarea>
        <span class="hint" class:bad={descLen > MAX_DESC}>{descLen} / {MAX_DESC} · {descTip || "C'est ce qui permet à Lama de choisir ce skill parmi les autres."}</span>
      </label>

      <label class="field">
        <span class="label">Instructions</span>
        <textarea class="mono" rows="12" bind:value={body} spellcheck="false" placeholder={"# Objectif\n\n# Étapes\n1. …\n\n# Format de la réponse"}></textarea>
        <span class="hint" class:bad={body.length > MAX_BODY}>Écris <code>$ARGUMENTS</code> là où Lama doit mettre ce que tu tapes après la commande. {body.length > 20000 ? `${body.length} / ${MAX_BODY} caractères.` : ""}</span>
      </label>

      <button class="link" onclick={() => (showOptions = !showOptions)} aria-expanded={showOptions}>{showOptions ? "Masquer" : "Afficher"} les options</button>
      {#if showOptions}
        <div class="options">
          <label class="check">
            <input type="checkbox" bind:checked={modelInvocable} />
            <span><b>Lama peut le choisir tout seul</b><br /><span class="muted">Décoche pour le réserver à la commande <code>/{slug || "nom"}</code> : utile pour une procédure qu'on ne veut jamais lancer par surprise.</span></span>
          </label>
          <label class="field">
            <span class="label">Ce qu'on tape après la commande</span>
            <input bind:value={argHint} placeholder="ex. [le texte à relire]" />
            <span class="hint">Seulement une indication affichée dans la liste des commandes.</span>
          </label>
          <div class="field">
            <span class="label">Où le ranger</span>
            <label class="radio"><input type="radio" bind:group={scope} value="app" disabled={!!editing} /> Pour tous mes projets</label>
            <label class="radio"><input type="radio" bind:group={scope} value="project" disabled={!!editing || !hasProject} /> Seulement pour ce projet{hasProject ? "" : " (ouvre un dossier en mode Code)"}</label>
          </div>
        </div>
      {/if}
    {/if}

    {#if error}<p class="bad" role="alert">{error}</p>{/if}
    <div class="row end">
      <span class="muted grow">{problems.length && !loading && dirty ? problems[0] : "Ctrl + Entrée pour enregistrer"}</span>
      <button onclick={cancel}>Annuler</button>
      <button class="primary" onclick={save} disabled={saving || loading || problems.length > 0}>{saving ? "Enregistrement…" : editing ? "Enregistrer" : "Créer le skill"}</button>
    </div>
  </div>
</div>

<style>
  .editor { max-width: 660px; width: calc(100vw - 32px); max-height: calc(100vh - 48px); overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  h3 { margin: 0; font-size: 16px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .bad { color: var(--del); font-size: 13px; margin: 0; }
  code { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12.5px; }
  .label { font-size: 13px; font-weight: 600; }
  .field { display: flex; flex-direction: column; gap: 5px; }
  .hint { font-size: 12.5px; color: var(--muted); line-height: 1.4; }
  .hint.bad { color: var(--del); }
  input:not([type="checkbox"]):not([type="radio"]), textarea { border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 8px; padding: 8px 10px; outline: none; font: inherit; font-size: 14px; width: 100%; box-sizing: border-box; }
  textarea { resize: vertical; line-height: 1.5; }
  textarea.mono { font-family: var(--mono); font-size: 13px; }
  input:focus, textarea:focus { border-color: var(--accent); }
  .start { display: flex; flex-direction: column; gap: 8px; padding: 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--code); }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 4px 12px; font-size: 13px; color: var(--fg); }
  .chip.on { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  .idea { display: flex; gap: 8px; }
  .idea input { flex: 1; }
  .options { display: flex; flex-direction: column; gap: 12px; padding: 12px; border: 1px solid var(--border); border-radius: 10px; }
  .check { display: flex; gap: 10px; align-items: flex-start; font-size: 14px; }
  .radio { display: flex; gap: 8px; align-items: center; font-size: 14px; }
  .row { display: flex; gap: 8px; align-items: center; }
  .row.end { justify-content: flex-end; }
  .grow { flex: 1; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); font: inherit; font-size: 13.5px; cursor: pointer; }
  button:disabled { opacity: 0.45; cursor: default; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .link { align-self: flex-start; border: 0; background: none; color: var(--accent); padding: 0; text-decoration: underline; }
</style>
