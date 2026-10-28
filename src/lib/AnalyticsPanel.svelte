<script lang="ts">
  import { onMount } from "svelte";
  import { analytics } from "$lib/analytics.svelte";

  let error = $state("");

  onMount(() => {
    void analytics.load();
    const timer = setInterval(() => analytics.load(), 5000);
    return () => clearInterval(timer);
  });

  async function toggle(e: Event) {
    const on = (e.currentTarget as HTMLInputElement).checked;
    try {
      error = "";
      await analytics.set(on);
    } catch (err) {
      error = String(err);
      await analytics.load();
    }
  }

  const when = (t: number) => new Date(t * 1000).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  const props = (p: Record<string, unknown>) => Object.entries(p).map(([k, v]) => `${k} : ${v}`).join(" · ");
  const NAMES: Record<string, string> = {
    app_started: "Lancement",
    chat_started: "Message envoyé",
    agent_started: "Tâche de l'agent",
    local_model_installed: "Modèle local installé",
    local_model_failed: "Installation de modèle échouée",
    connector_added: "Connecteur ajouté",
  };
</script>

<section>
  <p class="muted">Pour savoir quoi améliorer, Lama peut envoyer quelques chiffres anonymes sur la façon dont il est utilisé. C'est désactivé tant que tu n'as pas dit oui, et tu peux changer d'avis ici à tout moment.</p>

  {#if analytics.info && !analytics.info.available}
    <p class="note" role="status">Cette version n'a pas de service de statistiques configuré : rien n'est envoyé, quoi que tu coches.</p>
  {/if}

  <label class="check">
    <input type="checkbox" checked={analytics.info?.enabled ?? false} disabled={!analytics.info?.available} onchange={toggle} />
    <span><strong>Aider à améliorer Lama avec des statistiques anonymes</strong><br />
      <span class="muted">Aucun nom, aucune adresse e-mail, aucun identifiant gardé d'un lancement à l'autre.</span></span>
  </label>
  {#if error}<p class="bad" role="alert">{error}</p>{/if}

  <h3>Ce qui peut être envoyé</h3>
  <ul class="events">
    {#each analytics.info?.events ?? [] as [name, desc] (name)}
      <li><strong>{NAMES[name] ?? name}</strong><span class="muted"> — {desc}</span></li>
    {/each}
  </ul>
  <p class="muted">Avec chaque événement partent aussi la version de Lama Desktop, Windows et la date. Le service (Aptabase) déduit le pays à partir de la connexion et, d'après lui, n'enregistre pas l'adresse IP.</p>

  <h3>Ce qui ne part jamais</h3>
  <p class="muted">Le texte de tes conversations, tes fichiers, tes mémoires, tes clés et mots de passe, les noms de tes projets, le nom de ton PC ou de ton compte Windows.</p>

  <h3>Ce qui a été envoyé depuis le lancement</h3>
  <ul class="log">
    {#each analytics.info?.sent ?? [] as s (s.t + s.name + props(s.props))}
      <li><span class="when">{when(s.t)}</span><strong>{NAMES[s.name] ?? s.name}</strong><span class="muted">{props(s.props)}</span></li>
    {:else}
      <li class="muted">Rien pour l'instant.</li>
    {/each}
  </ul>
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 6px 0 0; font-size: 14px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .note { margin: 0; font-size: 13px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--code); }
  .bad { margin: 0; font-size: 13px; color: var(--del); }
  .check { display: flex; gap: 10px; align-items: flex-start; font-size: 14px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; font-size: 13.5px; }
  .log { font-size: 12.5px; max-height: 200px; overflow-y: auto; }
  .log li { display: flex; gap: 8px; flex-wrap: wrap; }
  .when { color: var(--muted); font-variant-numeric: tabular-nums; }
</style>
