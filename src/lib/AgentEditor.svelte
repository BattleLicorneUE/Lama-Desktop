<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { ChatStore } from "$lib/chat.svelte";
  import { answerConfirm, askConfirm, confirmState } from "$lib/dialogs.svelte";
  import { ACCESS, AGENT_TEMPLATES, AGENT_WRITER_PROMPT, accessOf, parseAgentReply, type Access } from "$lib/agent-templates";
  import { slugify } from "$lib/skill-templates";

  /** `from` : l'agent à modifier ; `copy` : un agent intégré dont on fait sa propre version. */
  let { chat, from, copy = false, onclose, onsaved }: { chat: ChatStore; from: string | null; copy?: boolean; onclose: () => void; onsaved: (message: string) => void } = $props();

  type Loaded = { draft: { name: string; description: string; prompt: string; model: string; tools: string[] | null }; scope: "builtin" | "app" | "project" };

  const MAX_DESC = 500;
  const hasProject = $derived(chat.mode === "code" && !!chat.workspace);
  const workspace = $derived(chat.mode === "code" && chat.workspace ? chat.workspace : null);

  let name = $state("");
  let description = $state("");
  let prompt = $state("");
  let model = $state("");
  let tools = $state<string[] | null>(null);
  let access = $state<Access>("all");
  let scope = $state<"app" | "project">("app");
  let idea = $state("");
  let template = $state("");
  let drafting = $state(false);
  let saving = $state(false);
  let error = $state("");
  // svelte-ignore state_referenced_locally
  let loading = $state(!!from);
  let initial = $state("");
  // Modifier un agent existant (pas une copie) : le nom d'origine, pour le renommer proprement.
  // svelte-ignore state_referenced_locally
  const original = copy ? null : from;

  const snapshot = () => JSON.stringify([name, description, prompt, model, tools]);
  const dirty = $derived(snapshot() !== initial);

  onMount(async () => {
    if (from) {
      try {
        const r = await invoke<Loaded>("agent_get", { workspace, name: from });
        name = copy ? `${r.draft.name}-perso` : r.draft.name;
        description = r.draft.description;
        prompt = r.draft.prompt;
        model = r.draft.model;
        tools = r.draft.tools;
        access = accessOf(tools);
        scope = r.scope === "project" ? "project" : "app";
      } catch (e) {
        error = String(e);
      }
      loading = false;
    }
    initial = copy ? "" : snapshot();
  });

  const slug = $derived(slugify(name));
  const taken = $derived(!!slug && chat.agents.some((a) => a.name.toLowerCase() === slug && a.name.toLowerCase() !== (original ?? "").toLowerCase()));
  const descLen = $derived(description.trim().length);
  const problems = $derived.by(() => {
    const p: string[] = [];
    if (!slug) p.push("Donne un nom à l'agent.");
    else if (taken) p.push(`Un agent « ${slug} » existe déjà.`);
    if (descLen < 8) p.push("Explique quand lui confier une tâche (une phrase suffit).");
    else if (descLen > MAX_DESC) p.push("La description est trop longue.");
    if (prompt.trim().length < 10) p.push("Écris la consigne de l'agent.");
    return p;
  });
  const modelChoices = $derived.by(() => {
    const list = chat.models.map((m) => ({ name: m.name, label: m.label ?? m.name }));
    return model && !list.some((m) => m.name === model) ? [{ name: model, label: `${model} (non disponible)` }, ...list] : list;
  });
  const accessHelp = $derived(access === "custom" ? `Cet agent a ${tools?.length ?? 0} outils choisis à la main : ils sont conservés.` : ACCESS.find((a) => a.id === access)?.help ?? "");

  function pickAccess(id: Exclude<Access, "custom">) {
    access = id;
    tools = ACCESS.find((a) => a.id === id)?.tools ?? null;
  }

  async function pickTemplate(id: string) {
    const t = AGENT_TEMPLATES.find((x) => x.id === id);
    if (!t) return;
    if (dirty && (name || description || prompt) && !(await askConfirm("Remplacer ce que tu as déjà écrit par ce modèle ?", "Remplacer"))) return;
    template = id;
    name = t.draft.name;
    description = t.draft.description;
    prompt = t.draft.prompt;
    tools = t.draft.tools;
    access = accessOf(tools);
  }

  async function writeWithLama() {
    error = "";
    if (!idea.trim()) return;
    if (!chat.model) {
      error = "Choisis d'abord un modèle dans la conversation : c'est lui qui rédige l'agent.";
      return;
    }
    if ((description.trim() || prompt.trim()) && !(await askConfirm("Lama va remplacer la description et la consigne que tu as écrites. Continuer ?", "Remplacer"))) return;
    drafting = true;
    try {
      const out = await invoke<string>("chat_once", {
        model: chat.model,
        cloudKey: chat.webKey,
        options: { temperature: 0.4 },
        messages: [
          { role: "system", content: AGENT_WRITER_PROMPT },
          { role: "user", content: idea.trim() },
        ],
      });
      const d = parseAgentReply(out);
      if (!d) {
        error = "Lama n'a pas réussi à rédiger un agent exploitable. Reformule ton idée ou réessaie.";
        return;
      }
      if (!name.trim() && d.name) name = d.name;
      description = (d.description ?? "").trim();
      prompt = (d.prompt ?? "").trim();
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
      await invoke("agent_save", { workspace, draft: { name, description, prompt, model, tools }, scope, original });
      await chat.loadAgents();
      onsaved(original ? `Agent « ${slug} » enregistré.` : `Agent « ${slug} » créé. Lama pourra lui confier du travail quand la description correspond.`);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function cancel() {
    if (dirty && !(await askConfirm("Abandonner les changements de cet agent ?", "Abandonner"))) return;
    onclose();
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      e.preventDefault();
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
  <div class="dialog editor" role="dialog" aria-modal="true" aria-label={original ? `Modifier l'agent ${original}` : copy ? "Copier un agent intégré" : "Nouvel agent"}>
    <h3>{original ? `Modifier « ${original} »` : copy ? `Ta version de « ${from} »` : "Nouvel agent"}</h3>
    <p class="muted">Un agent, c'est un rôle spécialisé : des consignes et des outils autorisés. Lama lui confie une partie du travail quand la description correspond à la tâche.</p>

    {#if loading}
      <p class="muted">Chargement…</p>
    {:else}
      {#if !original && !copy}
        <div class="start">
          <div class="label">Pour commencer</div>
          <div class="chips" role="group" aria-label="Modèles de départ">
            {#each AGENT_TEMPLATES as t (t.id)}
              <button class="chip" class:on={template === t.id} onclick={() => pickTemplate(t.id)}>{t.label}</button>
            {/each}
          </div>
          <form class="idea" onsubmit={(e) => { e.preventDefault(); void writeWithLama(); }}>
            <input bind:value={idea} placeholder="…ou décris ton idée : « un agent qui vérifie les traductions anglais-français »" disabled={drafting} aria-label="Ton idée d'agent" />
            <button class="primary" disabled={!idea.trim() || drafting}>{drafting ? "Rédaction…" : "Rédiger avec Lama"}</button>
          </form>
        </div>
      {/if}

      <label class="field">
        <span class="label">Nom</span>
        <input bind:value={name} placeholder="ex. relecteur-securite" spellcheck="false" aria-invalid={taken} />
        <span class="hint" class:bad={taken}>{taken ? `« ${slug} » existe déjà.` : "Des lettres, des chiffres et des tirets."}</span>
      </label>

      <label class="field">
        <span class="label">Quand lui confier une tâche ?</span>
        <textarea rows="2" bind:value={description} placeholder="Confie-lui une relecture de code axée sécurité : failles, secrets oubliés, entrées non vérifiées."></textarea>
        <span class="hint" class:bad={descLen > MAX_DESC}>{descLen} / {MAX_DESC} · C'est ce qui permet à Lama de le choisir parmi les autres.</span>
      </label>

      <label class="field">
        <span class="label">Consigne</span>
        <textarea class="mono" rows="9" bind:value={prompt} spellcheck="false" placeholder="Tu es … Ta méthode : … Ta réponse : …"></textarea>
        <span class="hint">Son rôle, sa méthode et le format de sa réponse, à la deuxième personne.</span>
      </label>

      <div class="field">
        <span class="label" id="acc">Ce qu'il peut faire</span>
        <div class="chips" role="radiogroup" aria-labelledby="acc">
          {#each ACCESS as a (a.id)}
            <button class="chip" role="radio" aria-checked={access === a.id} class:on={access === a.id} onclick={() => pickAccess(a.id)}>{a.label}</button>
          {/each}
          {#if access === "custom"}<button class="chip on" role="radio" aria-checked="true">Sur mesure</button>{/if}
        </div>
        <span class="hint">{accessHelp}</span>
      </div>

      <div class="grid">
        <label class="field">
          <span class="label">Modèle</span>
          <select bind:value={model}>
            <option value="">Celui de la conversation</option>
            {#each modelChoices as m (m.name)}<option value={m.name}>{m.label}</option>{/each}
          </select>
          <span class="hint">Un modèle plus petit suffit souvent pour une tâche simple.</span>
        </label>
        <div class="field">
          <span class="label">Où le ranger</span>
          <label class="radio"><input type="radio" bind:group={scope} value="app" disabled={!!original} /> Pour tous mes projets</label>
          <label class="radio"><input type="radio" bind:group={scope} value="project" disabled={!!original || !hasProject} /> Seulement ce projet{hasProject ? "" : " (mode Code)"}</label>
        </div>
      </div>
    {/if}

    {#if error}<p class="bad" role="alert">{error}</p>{/if}
    <div class="row end">
      <span class="muted grow">{problems.length && !loading && dirty ? problems[0] : "Ctrl + Entrée pour enregistrer"}</span>
      <button onclick={cancel}>Annuler</button>
      <button class="primary" onclick={save} disabled={saving || loading || problems.length > 0}>{saving ? "Enregistrement…" : original ? "Enregistrer" : "Créer l'agent"}</button>
    </div>
  </div>
</div>

<style>
  .editor { max-width: 660px; width: calc(100vw - 32px); max-height: calc(100vh - 48px); overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  h3 { margin: 0; font-size: 16px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .bad { color: var(--del); font-size: 13px; margin: 0; }
  .label { font-size: 13px; font-weight: 600; }
  .field { display: flex; flex-direction: column; gap: 5px; }
  .hint { font-size: 12.5px; color: var(--muted); line-height: 1.4; }
  .hint.bad { color: var(--del); }
  input:not([type="checkbox"]):not([type="radio"]), textarea, select { border: 1px solid var(--border); background: var(--surface); color: var(--fg); border-radius: 8px; padding: 8px 10px; outline: none; font: inherit; font-size: 14px; width: 100%; box-sizing: border-box; }
  textarea { resize: vertical; line-height: 1.5; }
  textarea.mono { font-family: var(--mono); font-size: 13px; }
  input:focus, textarea:focus, select:focus { border-color: var(--accent); }
  .start { display: flex; flex-direction: column; gap: 8px; padding: 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--code); }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 4px 12px; font-size: 13px; color: var(--fg); }
  .chip.on { background: var(--fg); color: var(--bg); border-color: var(--fg); }
  .idea { display: flex; gap: 8px; }
  .idea input { flex: 1; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .radio { display: flex; gap: 8px; align-items: center; font-size: 14px; }
  .row { display: flex; gap: 8px; align-items: center; }
  .row.end { justify-content: flex-end; }
  .grow { flex: 1; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); font: inherit; font-size: 13.5px; cursor: pointer; }
  button:disabled { opacity: 0.45; cursor: default; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
</style>
