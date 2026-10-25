<script lang="ts">
  import type { ChatStore, ToolEvent } from "$lib/chat.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { toolLabel } from "$lib/tool-labels";

  let { chat, tool, more }: { chat: ChatStore; tool: ToolEvent; more: number } = $props();

  const a = $derived(tool.args as Record<string, unknown>);
  const str = (v: unknown) => String(v ?? "");
  // Souris : la dernière capture vue par le modèle, avec un repère à l'endroit visé.
  type Shot = { data: string; width: number; height: number };
  let shot = $state<Shot | null>(null);
  const aims = $derived(tool.name === "computer" && a.x !== undefined && a.y !== undefined && ["click", "double_click", "right_click", "move"].includes(str(a.action)));
  onMount(() => {
    if (aims) invoke<Shot | null>("last_shot").then((s) => (shot = s)).catch(() => {});
  });
  const diffLines = $derived(tool.diff ? tool.diff.split("\n") : []);

  /** La demande en une phrase claire, puis le détail à lire avant d'accepter. */
  const info = $derived.by((): { question: string; detail?: string; mono?: boolean } => {
    switch (tool.name) {
      case "run_command":
        return { question: a.background ? "Lancer cette commande en arrière-plan ?" : "Exécuter cette commande ?", detail: str(a.command), mono: true };
      case "verify":
        return { question: "Lancer la vérification du projet ?", detail: str(a.target || "check") };
      case "write_file":
        return { question: "Créer ou remplacer ce fichier ?", detail: str(a.path) };
      case "edit_file":
        return { question: "Modifier ce fichier ?", detail: str(a.path) };
      case "make_dir":
        return { question: "Créer ce dossier ?", detail: str(a.path) };
      case "move_path":
        return { question: "Déplacer ou renommer ?", detail: `${str(a.from)}\n→ ${str(a.to)}` };
      case "delete_path":
        return { question: "Supprimer (vers la corbeille Windows) ?", detail: str(a.path) };
      case "open_target":
        return { question: "Ouvrir sur ton PC ?", detail: str(a.target) };
      case "clipboard":
        return { question: a.action === "write" ? "Copier ce texte dans le presse-papiers ?" : "Lire ton presse-papiers ?", detail: a.action === "write" ? str(a.text) : undefined };
      case "pc_control":
        return { question: "Modifier un réglage du PC ?", detail: [str(a.action), a.value !== undefined ? `valeur : ${str(a.value)}` : "", a.on !== undefined ? `activé : ${str(a.on)}` : ""].filter(Boolean).join(" · ") };
      case "screenshot":
        return { question: "Prendre une capture de ton écran et la lui montrer ?" };
      case "computer": {
        const where = a.x !== undefined ? ` en (${str(a.x)}, ${str(a.y)})` : "";
        const what = a.text ? ` : « ${str(a.text)} »` : a.keys ? ` : ${str(a.keys)}` : "";
        return { question: "Utiliser la souris ou le clavier ?", detail: `${str(a.action)}${where}${what}` };
      }
      case "create_document":
        return { question: "Créer ce document dans Documents\\Lama Desktop ?", detail: `${str(a.title)} (.${str(a.format)})` };
      case "vault_fill": {
        const items = (a.items as { kind: string; label: string; site?: string; username?: string }[] | undefined) ?? [];
        const what: Record<string, string> = { login: "Identifiant et mot de passe", username: "Identifiant", password: "Mot de passe", info: "Information" };
        const lines = items.map((i) => `${what[i.kind] ?? i.kind} — ${i.label}${i.site ? ` (${i.site})` : ""}${i.username ? ` · ${i.username}` : ""}`);
        return {
          question: "Saisir ceci au clavier dans la fenêtre active ?",
          detail: `${lines.join("\n")}\n\nLa valeur est tapée directement : le modèle ne la voit jamais. Vérifie que tu es sur le bon site, curseur dans le bon champ.${a.submit ? "\nEntrée sera pressée à la fin." : ""}`,
        };
      }
      case "vault_login": {
        const page = a.page as { url?: string; title?: string } | null;
        const what = a.mode === "password" ? "Mot de passe" : a.mode === "username" ? "Identifiant" : "Identifiant et mot de passe";
        const ok = a.allowed === true;
        return {
          question: "Se connecter avec le coffre dans le navigateur de Lama ?",
          detail:
            `${what} — ${str(a.label)} (${str(a.site)})${a.username ? ` · ${str(a.username)}` : ""}\n\n` +
            `Page ouverte : ${page ? `${page.title ?? ""} — ${page.url ?? ""}` : "aucune"}\n` +
            (ok ? "✔ L'adresse de la page correspond à l'entrée." : `✖ ${str(a.why) || "L'adresse de la page ne correspond pas à l'entrée."}\nLa saisie sera refusée.`) +
            `${a.submit ? "\nEntrée sera pressée pour valider." : ""}\n\nLe modèle ne voit jamais le mot de passe.`,
        };
      }
      case "browser_open":
        return { question: "Ouvrir cette page dans le navigateur de Lama ?", detail: str(a.url), mono: true };
      case "browser_read":
        return { question: "Lire la page affichée dans le navigateur de Lama ?" };
      case "browser_click":
        return { question: "Cliquer sur cet élément de la page ?", detail: `Élément [${str(a.index)}] — relis la page si tu hésites` };
      case "browser_type":
        return { question: "Saisir ce texte dans la page ?", detail: `Champ [${str(a.index)}] : ${str(a.text)}${a.submit ? "\nEntrée sera pressée." : ""}` };
      case "web_fetch":
        return { question: "Lire cette page web ?", detail: str(a.url), mono: true };
      case "web_search":
        return { question: "Chercher ceci sur le web ?", detail: str(a.query) };
      case "delegate":
        return { question: "Confier cette tâche à un agent ?", detail: `${str(a.agent)} — ${str(a.task)}` };
      default:
        return tool.name.startsWith("mcp__")
          ? { question: "Utiliser cet outil externe (MCP) ?", detail: JSON.stringify(tool.args, null, 2), mono: true }
          : { question: "Autoriser cette action ?", detail: JSON.stringify(tool.args, null, 2), mono: true };
    }
  });
</script>

<div class="overlay" role="presentation" onkeydown={(e) => e.key === "Escape" && chat.approve(tool.call_id, false)}>
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="ask-title" tabindex="-1">
    <p class="kicker">{tool.agent ? `${tool.agent} · ` : ""}{toolLabel(tool.name)}</p>
    <h2 id="ask-title">{info.question}</h2>
    {#if info.detail}<pre class:mono={info.mono}>{info.detail}</pre>{/if}
    {#if aims && shot}
      <div class="shot" aria-label="Aperçu de l'endroit visé">
        <img src={`data:image/png;base64,${shot.data}`} alt="Dernière capture d'écran" />
        <span class="aim" style:left={`${(Number(a.x) / shot.width) * 100}%`} style:top={`${(Number(a.y) / shot.height) * 100}%`}></span>
      </div>
    {/if}
    {#if diffLines.length}
      <pre class="diff" aria-label="Différences">{#each diffLines as l}<span class:add={l.startsWith("+") && !l.startsWith("+++")} class:del={l.startsWith("-") && !l.startsWith("---")} class:hunk={l.startsWith("@@")}>{l || " "}</span>{/each}</pre>
    {/if}
    {#if more > 0}<p class="more">{more} autre{more > 1 ? "s" : ""} demande{more > 1 ? "s" : ""} en attente après celle-ci.</p>{/if}
    <div class="row">
      <button class="no" onclick={() => chat.approve(tool.call_id, false)}>Refuser</button>
      <button class="yes" onclick={() => chat.approve(tool.call_id, true)}>Autoriser</button>
    </div>
  </div>
</div>

<style>
  .overlay { z-index: 90; }
  .dialog { max-width: 560px; display: flex; flex-direction: column; gap: 10px; }
  .kicker { margin: 0; color: var(--accent); font-size: 12.5px; font-weight: 600; letter-spacing: 0.02em; text-transform: uppercase; }
  h2 { margin: 0; font-family: var(--serif); font-weight: 400; font-size: 22px; line-height: 1.3; }
  pre { margin: 0; padding: 10px 12px; max-height: 260px; overflow: auto; background: var(--code); border-radius: 10px; font-size: 13px; line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; font-family: var(--sans); }
  pre.mono, pre.diff { font-family: ui-monospace, Consolas, monospace; font-size: 12.5px; }
  .diff { padding: 8px 0; white-space: normal; }
  .diff span { display: block; padding: 0 12px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .diff .add { background: var(--add-bg); color: var(--add); }
  .diff .del { background: var(--del-bg); color: var(--del); }
  .diff .hunk { color: var(--muted); }
  .shot { position: relative; border-radius: 10px; overflow: hidden; border: 1px solid var(--border); line-height: 0; }
  .shot img { width: 100%; display: block; }
  .aim { position: absolute; width: 34px; height: 34px; margin: -17px 0 0 -17px; border: 3px solid #e5322d; border-radius: 50%; box-shadow: 0 0 0 2px #fff, 0 0 12px rgb(0 0 0 / 0.5); pointer-events: none; }
  .aim::after { content: ""; position: absolute; inset: 11px; background: #e5322d; border-radius: 50%; }
  .more { margin: 0; color: var(--muted); font-size: 13px; }
  .row { display: flex; justify-content: flex-end; gap: 10px; margin-top: 4px; }
  .row button { border-radius: 10px; padding: 8px 20px; border: 1px solid var(--border); background: var(--surface); font-size: 14.5px; }
  .row .yes { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
</style>
