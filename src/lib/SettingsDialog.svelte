<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { COMPACT_AT, formatTokens, type ChatStore, type Theme } from "$lib/chat.svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "$lib/Icon.svelte";
  import Select from "$lib/Select.svelte";
  import MyProfilePanel from "$lib/MyProfilePanel.svelte";
  import SkillsPanel from "$lib/SkillsPanel.svelte";
  import AgentsPanel from "$lib/AgentsPanel.svelte";
  import ComputerPanel from "$lib/ComputerPanel.svelte";
  import ConnectorsCatalog from "$lib/ConnectorsCatalog.svelte";
  import BackgroundPanel from "$lib/BackgroundPanel.svelte";
  import UsagePanel from "$lib/UsagePanel.svelte";
  import ModelGuide from "$lib/ModelGuide.svelte";
  import DataPanel from "$lib/DataPanel.svelte";
  import ProgressPanel from "$lib/ProgressPanel.svelte";
  import VaultPanel from "$lib/VaultPanel.svelte";
  import UpdatePanel from "$lib/UpdatePanel.svelte";
  import HardwarePanel from "$lib/HardwarePanel.svelte";

  let { chat, profileName, onclose }: { chat: ChatStore; profileName: string; onclose: () => void } = $props();

  type Status = { name: string; tools: string[]; error: string | null };
  let servers = $state<Status[]>([]);
  let busy = $state(false);
  let keyInput = $state<HTMLInputElement>();
  type Tab = "general" | "account" | "profile" | "models" | "memory" | "vault" | "progress" | "usage" | "data" | "update" | "system" | "developer" | "skills" | "agents" | "connectors";
  // Ouverture directe sur une page (la progression depuis la puce de niveau) : lecture voulue, une seule fois.
  // svelte-ignore state_referenced_locally
  let tab = $state<Tab>((chat.settingsTab as Tab | null) ?? "general");
  let find = $state("");
  let newMemory = $state("");
  let msg = $state("");

  type Item = { id: Tab | "keys"; label: string; icon: string };
  const GROUPS: { label: string; items: Item[] }[] = [
    {
      label: "Paramètres",
      items: [
        { id: "general", label: "Général", icon: "settings" },
        { id: "account", label: "Compte", icon: "shield" },
        { id: "profile", label: "Mon profil", icon: "pencil" },
        { id: "models", label: "Modèles et contexte", icon: "gauge" },
        { id: "memory", label: "Mémoire", icon: "book" },
        { id: "vault", label: "Coffre", icon: "lock" },
        { id: "progress", label: "Progression", icon: "trophy" },
        { id: "usage", label: "Utilisation", icon: "gauge" },
        { id: "data", label: "Données et sauvegarde", icon: "download" },
        { id: "update", label: "Mises à jour", icon: "download" },
      ],
    },
    {
      label: "Cet ordinateur",
      items: [
        { id: "system", label: "Système", icon: "folder" },
        { id: "developer", label: "Développeur", icon: "file" },
      ],
    },
    {
      label: "Personnalisation",
      items: [
        { id: "skills", label: "Skills", icon: "check" },
        { id: "agents", label: "Agents", icon: "globe" },
        { id: "connectors", label: "Connecteurs", icon: "plus" },
      ],
    },
    { label: "Plateforme", items: [{ id: "keys", label: "Clés API", icon: "external" }] },
  ];
  const visible = (items: Item[]) => items.filter((i) => !find.trim() || i.label.toLowerCase().includes(find.trim().toLowerCase()));

  function go(it: Item) {
    if (it.id === "keys") {
      openUrl("https://ollama.com/settings/keys").catch(() => {});
      return;
    }
    tab = it.id;
    msg = "";
  }

  const TEXT_SIZES = [{ id: "small", label: "Petit" }, { id: "medium", label: "Moyen" }, { id: "large", label: "Grand" }] as const;
  const WIDTHS = [{ id: "narrow", label: "Étroit" }, { id: "medium", label: "Moyen" }, { id: "wide", label: "Large" }] as const;
  const FONTS = [{ id: "serif", label: "Classique" }, { id: "sans", label: "Sans empattement" }, { id: "mono", label: "Machine" }, { id: "readable", label: "Lisible" }] as const;
  const MOTIONS = [{ id: "system", label: "Système" }, { id: "reduced", label: "Réduites" }] as const;

  const THEMES: { id: Theme; label: string }[] = [
    { id: "system", label: "Système" },
    { id: "light", label: "Clair" },
    { id: "dark", label: "Sombre" },
  ];


  const SHORTCUTS = [
    ["Ctrl + N", "Nouveau chat"],
    ["Ctrl + K", "Rechercher une conversation"],
    ["Ctrl + M", "Gérer les modèles"],
    ["Ctrl + ,", "Réglages"],
    ["/", "Commandes et skills"],
    ["Entrée / Maj + Entrée", "Envoyer / nouvelle ligne"],
  ];

  onMount(async () => {
    chat.settingsTab = null;
    servers = await invoke<Status[]>("mcp_status");
    chat.loadSkills();
    chat.loadMemories();
  });

  async function reload() {
    busy = true;
    try {
      servers = await invoke<Status[]>("mcp_reload");
    } finally {
      busy = false;
    }
  }

  async function addMemory() {
    await chat.addMemory(newMemory);
    newMemory = "";
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="settings" role="dialog" aria-modal="true" aria-label="Réglages">
  <nav aria-label="Sections des réglages">
    <label class="find"><Icon name="search" size={16} /><input bind:value={find} placeholder="Rechercher" aria-label="Rechercher un réglage" /></label>
    {#each GROUPS as g}
      {@const items = visible(g.items)}
      {#if items.length}
        <p class="grp">{g.label}</p>
        {#each items as it}
          <button class:on={tab === it.id} onclick={() => go(it)}><Icon name={it.icon} size={17} /> <span>{it.label}</span></button>
        {/each}
      {/if}
    {/each}
  </nav>

  <main>
    <button class="x" onclick={onclose} aria-label="Fermer les réglages"><Icon name="close" size={20} /></button>
    <div class="page">
      {#if msg}<p class="msg">{msg}</p>{/if}

      {#if tab === "general"}
        <section>
          <h3>Apparence</h3>
          <div class="line">
            <div class="lab"><b>Thème</b></div>
            <div class="seg">
              {#each THEMES as t}<button class:on={chat.theme === t.id} onclick={() => chat.setTheme(t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Taille du texte de la conversation</b><small>Taille du texte des réponses.</small></div>
            <div class="seg">
              {#each TEXT_SIZES as t}<button class:on={chat.textSize === t.id} onclick={() => chat.setDisplay("textSize", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Largeur de la conversation</b><small>Largeur maximale de la colonne de messages et de la zone de saisie.</small></div>
            <div class="seg">
              {#each WIDTHS as t}<button class:on={chat.convWidth === t.id} onclick={() => chat.setDisplay("convWidth", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Police des réponses</b><small>« Lisible » : Verdana, plus espacée, plus facile à lire pour certains lecteurs (dyslexie).</small></div>
            <div class="seg">
              {#each FONTS as t}<button class:on={chat.font === t.id} onclick={() => chat.setDisplay("font", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Animations</b><small>Réduis les animations de l'interface et de l'affichage progressif des réponses.</small></div>
            <div class="seg">
              {#each MOTIONS as t}<button class:on={chat.motion === t.id} onclick={() => chat.setDisplay("motion", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
        </section>

        <section>
  <h3>Notifications</h3>
  <label class="check">
    <input type="checkbox" checked={chat.notifications} onchange={(e) => chat.setNotifications(e.currentTarget.checked)} />
    Me prévenir quand une réponse est prête ou qu'une autorisation est demandée (fenêtre en arrière-plan)
  </label>
</section>

        <section>
  <h3>Revue de code</h3>
  <label class="check">
    <input type="checkbox" checked={chat.autoReview} onchange={(e) => chat.setAutoReview(e.currentTarget.checked)} />
    Relire automatiquement les changements à la fin d'une tâche de l'agent (mode Code, onglet « Revue »)
  </label>
  <p class="muted">Un appel de plus au modèle à chaque tâche qui modifie des fichiers.</p>
</section>

        <BackgroundPanel />

        <section>
          <h3>Tutoriel</h3>
          <p class="muted">Présentation, tour des fonctions et clé API.</p>
          <div class="row"><button onclick={() => { chat.tutorialOpen = true; onclose(); }}>Revoir le tutoriel</button></div>
        </section>

        <section>
  <h3>Raccourcis</h3>
  <dl>
    {#each SHORTCUTS as [k, d]}<dt><kbd>{k}</kbd></dt><dd>{d}</dd>{/each}
  </dl>
</section>
      {:else if tab === "account"}
        <section>
  <h3>Ollama Cloud (clé API)</h3>
  <p class="muted">L'app utilise uniquement l'API d'Ollama Cloud : aucun modèle n'est installé sur ce PC. Crée une clé sur
    <button class="linkbtn" onclick={() => openUrl("https://ollama.com/settings/keys").catch(() => {})}>ollama.com/settings/keys</button>.</p>
  <label class="field">
    Clé API Ollama
    <input type="password" value={chat.webKey} placeholder="Colle ta clé ici" autocomplete="off" spellcheck="false"
      onchange={(e) => chat.setWebKey(e.currentTarget.value)} bind:this={keyInput} />
  </label>
  <div class="row">
    <button class="primary" disabled={!keyInput?.value.trim() && !chat.webKey} onclick={() => chat.connectCloud(keyInput?.value || chat.webKey)}>
      {chat.cloudStatus === "checking" ? "Test…" : "Connecter et tester"}
    </button>
    {#if chat.webKey}<button onclick={() => chat.disconnectCloud()}>Déconnecter</button>{/if}
  </div>
  {#if chat.cloudStatus === "ok"}
    <p class="muted okline">Connecté : la clé est acceptée par Ollama Cloud · {chat.cloudModels.length} modèles disponibles.</p>
  {:else if chat.cloudStatus === "listed"}
    <p class="muted">Catalogue chargé ({chat.cloudModels.length} modèles), clé pas encore testée : clique sur « Connecter et tester ».</p>
  {:else if chat.cloudStatus === "error"}
    <p class="muted ctxnote bad">{chat.cloudError}</p>
  {/if}
  <p class="muted">La clé est rangée dans le coffre Windows de ce profil et n'est envoyée qu'à ollama.com.</p>
  <p class="muted">
    Recherche web : toujours active avec tous les modèles (cloud et locaux), via DuckDuckGo. Elle se désactive toute seule quand l'ordinateur est hors ligne
    {#if !chat.online}(<b>c'est le cas en ce moment</b>){/if}. DuckDuckGo n'a pas d'API officielle : s'il ne répond pas ou bloque l'application, la recherche se rabat sur Wikipédia (et le dit au modèle), faute de mieux. En mode Manuel, chaque page lue demande ton accord ; les adresses locales sont refusées.</p>
</section>
      {:else if tab === "profile"}
        {#key chat.onboardVersion}<MyProfilePanel {chat} defaultName={profileName} />{/key}
      {:else if tab === "models"}
        <section>
  <h3>Contexte et modèles</h3>
  <p class="muted">
    Contexte automatique : l'app lit la limite de chaque modèle chez Ollama
    ({chat.modelMaxCtx ? `${formatTokens(chat.modelMaxCtx)} jetons pour ${chat.model}` : "inconnue pour le modèle courant, 32 k par défaut"})
    et l'utilise en entier. Quand la conversation dépasse {Math.round(COMPACT_AT * 100)} % de cette limite, les anciens messages sont résumés tout seuls ; le bouton de jauge et <code>/compact</code> restent disponibles.
  </p>
  {#if chat.unavailable[chat.model]}
    <p class="muted ctxnote bad">« {chat.model} » n'est plus disponible sur Ollama Cloud (erreur 410) : choisis un autre modèle.</p>
  {/if}
  <label class="field">
    Modèle du mode Plan (réflexion)
    <Select
      value={chat.planModel}
      items={[{ value: "", label: "Le même que le modèle courant" }, ...chat.models.map((m) => ({ value: m.name, label: m.name }))]}
      onchange={(v) => chat.setPlanModel(v)}
    />
  </label>
  <p class="muted">Astuce : choisis un gros modèle pour planifier en mode Plan, puis un modèle plus léger pour exécuter.</p>
</section>
        <section>
  <h3>Guide des modèles</h3>
  <p class="muted">Quel modèle pour quel usage, et combien il coûte.</p>
  <ModelGuide {chat} />
</section>
      {:else if tab === "memory"}
        <section>
  <p class="muted">Ces informations sont transmises au modèle dans toutes les conversations. Ajoute-en avec <code>/remember …</code>, ici, ou laisse l'agent le faire ; <code>/memoire</code> condense la conversation en cours en quelques lignes.</p>
  <p class="muted">Ce que Lama sait de toi (usages, style de réponse…) se modifie dans l'onglet <button class="linkbtn" onclick={() => (tab = "profile")}>Profil</button>.</p>
  <form class="row" onsubmit={(e) => { e.preventDefault(); addMemory(); }}>
    <input class="grow" bind:value={newMemory} placeholder="Ex. Je préfère les réponses courtes et en français" />
    <button class="primary" disabled={!newMemory.trim()}>Ajouter</button>
  </form>
  <ul>
    {#each chat.memories as m (m.id)}
      <li><span class="grow">{m.text}</span>
        <button class="del" aria-label="Oublier" onclick={() => chat.deleteMemory(m.id)}><Icon name="close" size={14} /></button></li>
    {:else}
      <li class="muted">Rien en mémoire.</li>
    {/each}
  </ul>
</section>
      {:else if tab === "vault"}
        <VaultPanel />
      {:else if tab === "progress"}
        <ProgressPanel {chat} />
      {:else if tab === "usage"}
        <UsagePanel {chat} />
      {:else if tab === "data"}
        <DataPanel {chat} />
      {:else if tab === "update"}
        <UpdatePanel {chat} />
      {:else if tab === "system"}
        <ComputerPanel />
        <HardwarePanel {chat} />
      {:else if tab === "developer"}
        <section>
  <p class="muted">
    Format identique à Claude Desktop (<code>mcpServers</code>). Les outils MCP sont proposés à l'agent et demandent ton accord dans une fenêtre, sauf en mode « Ignorer les permissions ».
  </p>
  <div class="row">
    <button onclick={() => invoke("mcp_edit_config")}>Modifier la configuration</button>
    <button onclick={reload} disabled={busy}>{busy ? "Chargement…" : "Recharger"}</button>
  </div>
  <ul>
    {#each servers as s}
      <li><strong>{s.name}</strong>
        {#if s.error}<span class="err">{s.error}</span>{:else}<span class="muted">{s.tools.length} outil{s.tools.length > 1 ? "s" : ""}</span>{/if}</li>
    {:else}
      <li class="muted">Aucun serveur configuré.</li>
    {/each}
  </ul>
</section>
      {:else if tab === "skills"}
        <SkillsPanel {chat} />
      {:else if tab === "agents"}
        <AgentsPanel {chat} />
      {:else}
        <ConnectorsCatalog {servers} onreload={reload} />
      {/if}
    </div>
  </main>
</div>

<style>
  .settings { position: fixed; inset: 0; z-index: 50; display: grid; grid-template-columns: 250px minmax(0, 1fr); background: var(--bg); }
  nav { display: flex; flex-direction: column; gap: 2px; padding: 14px 10px; border-right: 1px solid var(--border); overflow-y: auto; background: var(--sidebar); }
  .find { display: flex; align-items: center; gap: 8px; border: 1px solid var(--border); background: var(--bg); border-radius: 10px; padding: 0 10px; color: var(--muted); margin-bottom: 8px; }
  .find input { flex: 1; min-width: 0; border: 0 !important; background: none !important; padding: 8px 0 !important; outline: none; color: var(--fg); font-size: 14.5px; }
  .grp { margin: 14px 10px 4px; font-size: 12.5px; color: var(--muted); }
  nav button { display: flex; align-items: center; gap: 10px; border: 0; background: none; color: var(--fg); font: inherit; font-size: 14.5px; padding: 8px 10px; border-radius: 9px; text-align: left; }
  nav button:hover, nav button.on { background: var(--hover); }
  nav button.on { font-weight: 600; }
  nav button span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  main { position: relative; overflow-y: auto; }
  .page { max-width: 780px; margin: 0 auto; padding: 44px 32px 60px; display: flex; flex-direction: column; gap: 30px; }
  .x { position: absolute; top: 14px; right: 18px; display: grid; place-items: center; width: 34px; height: 34px; border: 0; background: none; color: var(--fg); border-radius: 8px; }
  .x:hover { background: var(--hover); }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  section { display: flex; flex-direction: column; gap: 10px; }
  .line { display: flex; justify-content: space-between; align-items: center; gap: 20px; padding: 14px 0; border-bottom: 1px solid var(--border); }
  .lab { display: grid; gap: 3px; min-width: 0; }
  .lab b { font-weight: 500; font-size: 15px; }
  .lab small { color: var(--muted); font-size: 13.5px; }
  .seg { display: inline-flex; flex: none; background: var(--hover); border-radius: 10px; padding: 3px; }
  .seg button { border: 0; background: none; padding: 5px 14px; border-radius: 8px; color: var(--muted); }
  .seg button.on { background: var(--surface); color: var(--fg); box-shadow: 0 1px 2px rgb(0 0 0 / 0.12); }
  .check { display: flex; gap: 8px; align-items: flex-start; font-size: 14px; }
  .field { display: flex; flex-direction: column; gap: 5px; font-size: 13px; color: var(--muted); }
  .page input:not([type="checkbox"]) { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 7px 10px; outline: none; color: var(--fg); }
  .page input:focus { border-color: var(--accent); }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .ctxnote.bad { color: var(--del); }
  .okline { color: var(--add); }
  .linkbtn { border: 0 !important; background: none !important; padding: 0 !important; color: var(--accent); text-decoration: underline; }
  .msg { margin: 0; font-size: 13px; color: var(--accent); }
  code, kbd { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12.5px; }
  .row { display: flex; gap: 8px; }
  .grow { flex: 1; min-width: 0; }
  .page button:not(.seg button):not(.linkbtn):not(.del) { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); }
  button:disabled { opacity: 0.45; }
  .primary { background: var(--accent) !important; color: var(--accent-fg) !important; border-color: var(--accent) !important; }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 10px; }
  li { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid var(--border); font-size: 14px; }
  li:last-child { border-bottom: 0; }
  .del { border: 0 !important; background: none !important; color: var(--muted); font-size: 16px; padding: 0 6px !important; }
  .err { color: var(--del); font-size: 13px; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 6px 14px; margin: 0; font-size: 14px; }
  dd { margin: 0; color: var(--muted); }
  dt { margin: 0; }
  @media (max-width: 760px) {
    .settings { grid-template-columns: 1fr; grid-template-rows: auto minmax(0, 1fr); }
    nav { flex-direction: row; flex-wrap: wrap; border-right: 0; border-bottom: 1px solid var(--border); max-height: 36vh; }
    .grp, .find { display: none; }
    .page { padding: 28px 18px 48px; }
  }
</style>
