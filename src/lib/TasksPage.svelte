<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import type { ChatStore } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";
  import { DAYS, REPEATS, dateInput, describeNext, describeRepeat, type Repeat, type ScheduledTask } from "$lib/schedule";
  import Icon from "$lib/Icon.svelte";
  import Select from "$lib/Select.svelte";

  let { chat }: { chat: ChatStore } = $props();

  type Draft = { id: string; name: string; prompt: string; kind: "chat" | "code"; workspace: string; project_id: string; repeat: Repeat; time: string; weekday: number; date: string };
  const blank = (): Draft => ({ id: "", name: "", prompt: "", kind: "chat", workspace: "", project_id: "", repeat: "daily", time: "08:00", weekday: 0, date: dateInput(new Date()) });
  let form = $state<Draft | null>(null);
  let error = $state("");
  let busy = $state(false);

  const projectsOfKind = $derived(chat.projects.filter((p) => p.kind === (form?.kind ?? "chat")));
  const projectName = (id: string | null) => chat.projects.find((p) => p.id === id)?.name;

  function edit(t: ScheduledTask) {
    error = "";
    form = { id: t.id, name: t.name, prompt: t.prompt, kind: t.kind, workspace: t.workspace ?? "", project_id: t.project_id ?? "", repeat: t.repeat, time: t.time, weekday: t.weekday, date: t.date ?? dateInput(new Date()) };
  }

  async function pickFolder() {
    const dir = await openDialog({ directory: true, multiple: false, title: "Dossier de travail de la tâche" });
    if (typeof dir === "string" && form) form.workspace = dir;
  }

  async function save() {
    if (!form) return;
    busy = true;
    error = "";
    try {
      const old = chat.tasks.find((t) => t.id === form!.id);
      await chat.saveTask({
        ...(old ? $state.snapshot(old) : {}),
        id: form.id,
        name: form.name,
        prompt: form.prompt,
        kind: form.kind,
        workspace: form.kind === "code" ? form.workspace.trim() || null : null,
        project_id: form.project_id || null,
        repeat: form.repeat,
        time: form.time || "08:00",
        weekday: form.weekday,
        date: form.repeat === "once" ? form.date : null,
        enabled: old?.enabled ?? true,
      });
      form = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function remove(t: ScheduledTask) {
    if (await askConfirm(`Supprimer la tâche « ${t.name} » ? Les conversations qu'elle a créées sont conservées.`)) await chat.deleteTask(t.id);
  }

  function ranAt(t: ScheduledTask) {
    return t.last_at ? new Date(t.last_at * 1000).toLocaleString("fr-FR", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" }) : null;
  }
</script>

<div class="page">
  <header>
    <div>
      <h1>Tâches planifiées</h1>
      <p class="sub">Une consigne lancée toute seule, à l'heure choisie, dans une nouvelle conversation.</p>
    </div>
    {#if !form}<button class="primary" onclick={() => { error = ""; form = blank(); }}><Icon name="plus" size={15} /> Nouvelle tâche</button>{/if}
  </header>

  <p class="note">
    Les tâches se lancent seulement quand Lama Desktop est ouvert (active « Garder actif en arrière-plan » dans Réglages → Général pour qu'il reste dans la zone de notification).
    Une tâche manquée se lance à la réouverture. Les actions qui demandent ton accord attendent ta validation, selon le mode d'autorisation choisi.
  </p>

  {#if form}
    <form class="card" onsubmit={(e) => { e.preventDefault(); save(); }}>
      <label>
        <span>Nom</span>
        <input bind:value={form.name} placeholder="Ex. Revue de presse du matin" autocomplete="off" />
      </label>
      <label>
        <span>Consigne</span>
        <textarea bind:value={form.prompt} rows="4" placeholder="Ex. Cherche sur le web les nouveautés sur Tauri et résume-les en 5 points."></textarea>
      </label>
      <div class="seg" role="radiogroup" aria-label="Type de tâche">
        <button type="button" role="radio" aria-checked={form.kind === "chat"} class:on={form.kind === "chat"} onclick={() => { form!.kind = "chat"; form!.project_id = ""; }}>Chat</button>
        <button type="button" role="radio" aria-checked={form.kind === "code"} class:on={form.kind === "code"} onclick={() => { form!.kind = "code"; form!.project_id = ""; }}>Code</button>
      </div>
      {#if form.kind === "code"}
        <label>
          <span>Dossier de travail</span>
          <span class="row">
            <input class="grow" bind:value={form.workspace} placeholder="C:\Users\…\mon-projet" autocomplete="off" spellcheck="false" />
            <button type="button" onclick={pickFolder}>Parcourir</button>
          </span>
        </label>
      {/if}
      {#if projectsOfKind.length}
        <label>
          <span>Projet (facultatif)</span>
          <Select value={form.project_id} items={[{ value: "", label: "Aucun projet" }, ...projectsOfKind.map((p) => ({ value: p.id, label: p.name }))]} onchange={(v) => (form!.project_id = v)} />
        </label>
      {/if}
      <div class="when">
        <label>
          <span>Répétition</span>
          <Select value={form.repeat} items={REPEATS.map((r) => ({ value: r.id, label: r.label }))} onchange={(v) => (form!.repeat = v as Repeat)} />
        </label>
        {#if form.repeat === "weekly"}
          <label>
            <span>Jour</span>
            <Select value={String(form.weekday)} items={DAYS.map((d, i) => ({ value: String(i), label: d }))} onchange={(v) => (form!.weekday = Number(v))} />
          </label>
        {/if}
        {#if form.repeat === "once"}
          <label><span>Date</span><input type="date" bind:value={form.date} /></label>
        {/if}
        <label>
          <span>{form.repeat === "hourly" ? "Minute (les heures sont ignorées)" : "Heure"}</span>
          <input type="time" bind:value={form.time} />
        </label>
      </div>
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <div class="row end">
        <button type="button" onclick={() => (form = null)}>Annuler</button>
        <button class="primary" disabled={busy || !form.name.trim() || !form.prompt.trim()}>{form.id ? "Enregistrer" : "Créer la tâche"}</button>
      </div>
    </form>
  {/if}

  <ul class="list">
    {#each chat.tasks as t (t.id)}
      <li class:off={!t.enabled}>
        <div class="main">
          <b>{t.name}</b>
          <span class="meta">
            {describeRepeat(t)}
            {#if t.enabled && t.next_at} · prochaine : {describeNext(t.next_at)}{:else if !t.enabled} · en pause{/if}
            {#if t.kind === "code"} · code{/if}{#if projectName(t.project_id)} · {projectName(t.project_id)}{/if}
          </span>
          <span class="prompt">{t.prompt}</span>
          {#if ranAt(t)}<span class="meta">Dernier lancement : {ranAt(t)}</span>{/if}
        </div>
        <div class="acts">
          <label class="switch" title={t.enabled ? "Mettre en pause" : "Réactiver"}>
            <input type="checkbox" checked={t.enabled} onchange={(e) => chat.setTaskEnabled(t, e.currentTarget.checked).catch((err) => (error = String(err)))} aria-label="Activer {t.name}" />
          </label>
          <button class="icon" onclick={() => chat.runTask(t, true)} title="Lancer maintenant" aria-label="Lancer {t.name} maintenant"><Icon name="play" size={16} /></button>
          <button class="icon" onclick={() => edit(t)} title="Modifier" aria-label="Modifier {t.name}"><Icon name="pencil" size={16} /></button>
          <button class="icon" onclick={() => remove(t)} title="Supprimer" aria-label="Supprimer {t.name}"><Icon name="trash" size={16} /></button>
        </div>
      </li>
    {:else}
      {#if !form}<li class="empty">Aucune tâche planifiée pour l'instant.</li>{/if}
    {/each}
  </ul>
  {#if error && !form}<p class="err" role="alert">{error}</p>{/if}
</div>

<style>
  .page { flex: 1; overflow: auto; padding: 36px max(24px, calc((100% - 880px) / 2)) 40px; display: flex; flex-direction: column; gap: 18px; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
  header .primary { flex: none; white-space: nowrap; }
  h1 { margin: 0 0 6px; font-size: 28px; font-weight: 500; font-family: Georgia, "Times New Roman", serif; }
  .sub { margin: 0; color: var(--muted); font-size: 15px; }
  .note { margin: 0; color: var(--muted); font-size: 13px; line-height: 1.5; }
  .card { display: flex; flex-direction: column; gap: 14px; border: 1px solid var(--border); background: var(--surface); border-radius: 14px; padding: 18px; }
  label { display: flex; flex-direction: column; gap: 5px; font-size: 13px; color: var(--muted); min-width: 0; }
  input, textarea { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 7px 10px; outline: none; color: var(--fg); font: inherit; font-size: 14px; }
  textarea { resize: vertical; }
  input:focus, textarea:focus { border-color: var(--accent); }
  .when { display: grid; grid-template-columns: repeat(auto-fit, minmax(170px, 1fr)); gap: 14px; }
  .row { display: flex; gap: 8px; }
  .row.end { justify-content: flex-end; }
  .grow { flex: 1; min-width: 0; }
  .seg { display: inline-flex; align-self: flex-start; background: var(--hover); border-radius: 10px; padding: 3px; }
  .seg button { border: 0; background: none; padding: 5px 16px; border-radius: 8px; color: var(--muted); }
  .seg button.on { background: var(--surface); color: var(--fg); box-shadow: 0 1px 2px rgb(0 0 0 / 0.12); }
  button { font: inherit; }
  .card .row button:not(.primary), header button:not(.primary) { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); }
  .primary { display: inline-flex; align-items: center; gap: 6px; border: 1px solid var(--accent); background: var(--accent); color: var(--accent-fg); border-radius: 8px; padding: 7px 14px; }
  .primary:disabled { opacity: 0.45; }
  .err { margin: 0; color: var(--del); font-size: 13.5px; }
  .list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 14px; }
  .list:empty { display: none; }
  li { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; padding: 14px 16px; border-bottom: 1px solid var(--border); }
  li:last-child { border-bottom: 0; }
  li.off .main { opacity: 0.55; }
  li.empty { color: var(--muted); justify-content: center; padding: 28px; }
  .main { display: grid; gap: 3px; min-width: 0; }
  .main b { font-weight: 500; font-size: 15.5px; }
  .meta { color: var(--muted); font-size: 13px; }
  .prompt { color: var(--muted); font-size: 13.5px; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .acts { display: flex; align-items: center; gap: 2px; flex: none; }
  .icon { display: grid; place-items: center; border: 0; background: none; color: var(--muted); width: 32px; height: 32px; border-radius: 8px; }
  .icon:hover { background: var(--hover); color: var(--fg); }
  .switch { flex-direction: row; margin-right: 6px; }
  .switch input { width: 36px; height: 20px; accent-color: var(--accent); }
</style>
