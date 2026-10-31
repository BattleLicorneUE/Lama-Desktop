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
  import AnalyticsPanel from "$lib/AnalyticsPanel.svelte";
  import HardwarePanel from "$lib/HardwarePanel.svelte";

  let { chat, profileName, onclose }: { chat: ChatStore; profileName: string; onclose: () => void } = $props();

  type Status = { name: string; tools: string[]; error: string | null };
  let servers = $state<Status[]>([]);
  let busy = $state(false);
  let keyInput = $state<HTMLInputElement>();
  type Tab =
    | "profile" | "memory" | "progress"
    | "appearance" | "general"
    | "account" | "local" | "models" | "web" | "usage"
    | "system" | "skills" | "agents" | "connectors"
    | "vault" | "data" | "stats" | "update";
  // Ouverture directe sur une page (la progression depuis la puce de niveau, les mises à jour depuis l'avis) : une seule fois.
  // svelte-ignore state_referenced_locally
  let tab = $state<Tab>((chat.settingsTab as Tab | null) ?? "appearance");
  let find = $state("");
  let newMemory = $state("");
  let msg = $state("");

  type Item = { id: Tab; label: string; icon: string; words: string };
  const GROUPS: { label: string; items: Item[] }[] = [
    {
      label: "Mon espace",
      items: [
        { id: "profile", label: "Mon profil", icon: "pencil", words: "nom style réponses ton langue questions" },
        { id: "memory", label: "Mémoire", icon: "book", words: "notes souvenirs retenir" },
        { id: "progress", label: "Progression", icon: "trophy", words: "niveau points badges série jeu" },
      ],
    },
    {
      label: "Application",
      items: [
        { id: "appearance", label: "Apparence", icon: "settings", words: "thème sombre clair police taille texte largeur animations" },
        { id: "general", label: "Préférences", icon: "settings", words: "notifications démarrage windows arrière-plan raccourcis tutoriel revue" },
      ],
    },
    {
      label: "Modèles et internet",
      items: [
        { id: "account", label: "Ollama Cloud", icon: "globe", words: "clé api token compte connexion" },
        { id: "local", label: "Modèles sur ce PC", icon: "gauge", words: "local hors ligne installer télécharger carte graphique mémoire contexte llama" },
        { id: "models", label: "Choisir un modèle", icon: "check", words: "guide prix usage plan contexte compactage" },
        { id: "web", label: "Recherche web", icon: "search", words: "internet duckduckgo wikipédia hors ligne" },
        { id: "usage", label: "Consommation", icon: "gauge", words: "jetons utilisation statistiques" },
      ],
    },
    {
      label: "Ce que Lama peut faire",
      items: [
        { id: "system", label: "Contrôle du PC", icon: "folder", words: "dossiers autorisés rappels journal souris clavier presse-papiers" },
        { id: "skills", label: "Skills", icon: "check", words: "consignes compétences commandes" },
        { id: "agents", label: "Agents", icon: "globe", words: "assistants spécialisés" },
        { id: "connectors", label: "Connecteurs", icon: "plus", words: "mcp outils serveurs github agenda" },
      ],
    },
    {
      label: "Sécurité et données",
      items: [
        { id: "vault", label: "Coffre", icon: "lock", words: "mots de passe infos rapides adresse téléphone navigateur" },
        { id: "data", label: "Sauvegarde et effacement", icon: "download", words: "export restaurer supprimer tout effacer" },
        { id: "stats", label: "Statistiques anonymes", icon: "gauge", words: "données confidentialité vie privée analytics envoyer aptabase télémétrie" },
      ],
    },
    { label: "À propos", items: [{ id: "update", label: "Mises à jour", icon: "download", words: "version nouveautés" }] },
  ];
  // Sans accents ni majuscules, et chaque mot tapé doit se retrouver quelque part : « mot de passe » trouve « mots de passe ».
  const plain = (t: string) => t.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
  const match = (i: Item, q: string) => {
    const hay = plain(`${i.label} ${i.words}`);
    return plain(q).split(/\s+/).filter(Boolean).every((w) => hay.includes(w));
  };
  const visible = (items: Item[]) => items.filter((i) => !find.trim() || match(i, find.trim()));

  /** Titre et phrase d'introduction de chaque page. */
  const PAGES: Record<Tab, { title: string; intro: string }> = {
    profile: { title: "Mon profil", intro: "Ce que Lama sait de toi pour répondre à ta façon. Modifie-le quand ta situation change." },
    memory: { title: "Mémoire", intro: "Les notes que Lama garde d'une conversation à l'autre." },
    progress: { title: "Progression", intro: "Ton niveau, tes séries et tes badges. Rien d'obligatoire, c'est juste pour le plaisir." },
    appearance: { title: "Apparence", intro: "Le thème, la taille du texte et la police. Tu vois le résultat tout de suite." },
    general: { title: "Préférences", intro: "Notifications, démarrage avec Windows, raccourcis clavier." },
    account: { title: "Ollama Cloud", intro: "La clé qui donne accès aux modèles hébergés par Ollama. Sans clé, tu peux quand même utiliser les modèles installés sur ton PC." },
    local: { title: "Modèles sur ce PC", intro: "Fais tourner une IA directement sur ton ordinateur, sans internet et sans clé. Ça dépend de la puissance de ton PC, alors Lama regarde ce qu'il a avant de te proposer quoi que ce soit." },
    models: { title: "Choisir un modèle", intro: "Quel modèle pour quel besoin, et ce qui se passe quand une conversation devient longue." },
    web: { title: "Recherche web", intro: "Lama peut chercher sur internet quand une réponse en a besoin." },
    usage: { title: "Consommation", intro: "Combien de texte tu as échangé avec les modèles, par jour et par modèle." },
    system: { title: "Contrôle du PC", intro: "Ce que Lama a le droit de faire sur ton ordinateur : ouvrir une appli, lire le presse-papiers, toucher à tes fichiers." },
    skills: { title: "Skills", intro: "Des consignes toutes prêtes que Lama suit pour une tâche précise, comme relire un texte ou préparer un mail." },
    agents: { title: "Agents", intro: "Des assistants avec leurs propres consignes et leurs propres droits. Lama peut leur confier une partie du travail." },
    connectors: { title: "Connecteurs", intro: "Relie Lama à d'autres outils (fichiers, agenda, bases de données…) grâce aux serveurs MCP." },
    vault: { title: "Coffre", intro: "Tes mots de passe et tes infos personnelles, chiffrés sur ce PC. Lama peut les remplir pour toi sans jamais les lire." },
    data: { title: "Sauvegarde et effacement", intro: "Sauvegarder tes données, les exporter, ou tout supprimer." },
    stats: { title: "Statistiques anonymes", intro: "Ce que Lama peut envoyer pour qu'on sache quoi améliorer, et comment le couper." },
    update: { title: "Mises à jour", intro: "La version installée et les nouveautés." },
  };

  // Statistiques anonymes (si acceptées) : quelles pages servent. Seul le nom de la page part.
  $effect(() => {
    void invoke("analytics_ui", { event: "settings_page", value: tab }).catch(() => {});
  });

  function go(it: Item) {
    tab = it.id;
    msg = "";
  }

  const TEXT_SIZES = [{ id: "small", label: "Petit" }, { id: "medium", label: "Moyen" }, { id: "large", label: "Grand" }] as const;
  const WIDTHS = [{ id: "narrow", label: "Étroit" }, { id: "medium", label: "Moyen" }, { id: "wide", label: "Large" }] as const;
  const FONTS = [{ id: "serif", label: "Classique" }, { id: "sans", label: "Sans empattement" }, { id: "mono", label: "Machine" }, { id: "readable", label: "Lisible" }] as const;
  const MOTIONS = [{ id: "system", label: "Comme Windows" }, { id: "reduced", label: "Réduites" }] as const;

  const THEMES: { id: Theme; label: string }[] = [
    { id: "system", label: "Comme Windows" },
    { id: "light", label: "Clair" },
    { id: "dark", label: "Sombre" },
  ];

  const SHORTCUTS = [
    ["Ctrl + N", "Nouvelle conversation"],
    ["Ctrl + K", "Chercher une conversation"],
    ["Ctrl + M", "Gérer les modèles"],
    ["Ctrl + ,", "Ouvrir les réglages"],
    ["/", "Commandes et skills"],
    ["Entrée", "Envoyer"],
    ["Maj + Entrée", "Aller à la ligne"],
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
    <label class="find"><Icon name="search" size={16} /><input bind:value={find} placeholder="Rechercher un réglage" aria-label="Rechercher un réglage" /></label>
    {#each GROUPS as g}
      {@const items = visible(g.items)}
      {#if items.length}
        <p class="grp">{g.label}</p>
        {#each items as it}
          <button class:on={tab === it.id} onclick={() => go(it)}><Icon name={it.icon} size={17} /> <span>{it.label}</span></button>
        {/each}
      {/if}
    {/each}
    {#if find.trim() && !GROUPS.some((g) => visible(g.items).length)}
      <p class="none">Aucun réglage ne correspond à « {find.trim()} ».</p>
    {/if}
  </nav>

  <main>
    <button class="x" onclick={onclose} aria-label="Fermer les réglages"><Icon name="close" size={20} /></button>
    <div class="page">
      <header class="ph">
        <h2>{PAGES[tab].title}</h2>
        <p>{PAGES[tab].intro}</p>
      </header>
      {#if msg}<p class="msg">{msg}</p>{/if}

      {#if tab === "appearance"}
        <section>
          <div class="line">
            <div class="lab"><b>Thème</b></div>
            <div class="seg">
              {#each THEMES as t}<button class:on={chat.theme === t.id} onclick={() => chat.setTheme(t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Taille du texte</b><small>Pour les réponses de Lama.</small></div>
            <div class="seg">
              {#each TEXT_SIZES as t}<button class:on={chat.textSize === t.id} onclick={() => chat.setDisplay("textSize", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Largeur de la conversation</b><small>Jusqu'où s'étendent les messages et la zone de saisie.</small></div>
            <div class="seg">
              {#each WIDTHS as t}<button class:on={chat.convWidth === t.id} onclick={() => chat.setDisplay("convWidth", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Police des réponses</b><small>« Lisible » utilise Verdana, plus espacée. Pratique si tu as du mal à lire, avec une dyslexie par exemple.</small></div>
            <div class="seg">
              {#each FONTS as t}<button class:on={chat.font === t.id} onclick={() => chat.setDisplay("font", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
          <div class="line">
            <div class="lab"><b>Animations</b><small>Moins de mouvement dans l'interface et quand une réponse s'affiche.</small></div>
            <div class="seg">
              {#each MOTIONS as t}<button class:on={chat.motion === t.id} onclick={() => chat.setDisplay("motion", t.id)}>{t.label}</button>{/each}
            </div>
          </div>
        </section>

      {:else if tab === "general"}
        <section>
          <h3>Notifications</h3>
          <label class="check">
            <input type="checkbox" checked={chat.notifications} onchange={(e) => chat.setNotifications(e.currentTarget.checked)} />
            <span>Me prévenir quand une réponse est prête ou quand Lama a besoin de mon accord, si la fenêtre est en arrière-plan.</span>
          </label>
        </section>

        <BackgroundPanel />

        <section>
          <h3>Relecture du code</h3>
          <label class="check">
            <input type="checkbox" checked={chat.autoReview} onchange={(e) => chat.setAutoReview(e.currentTarget.checked)} />
            <span>Relire les changements quand l'agent finit une tâche (mode Code, onglet « Revue »).</span>
          </label>
          <p class="muted">Ça ajoute un appel au modèle à chaque tâche qui modifie des fichiers.</p>
        </section>

        <section>
          <h3>Raccourcis clavier</h3>
          <dl>
            {#each SHORTCUTS as [k, d]}<dt><kbd>{k}</kbd></dt><dd>{d}</dd>{/each}
          </dl>
        </section>

        <section>
          <h3>Tutoriel</h3>
          <p class="muted">Le tour de l'application, depuis le début.</p>
          <div class="row"><button onclick={() => { chat.tutorialOpen = true; onclose(); }}>Revoir le tutoriel</button></div>
        </section>

      {:else if tab === "account"}
        <section>
          <p class="muted">
            Crée ta clé sur
            <button class="linkbtn" onclick={() => openUrl("https://ollama.com/settings/keys").catch(() => {})}>ollama.com/settings/keys</button>
            puis colle-la ici.
          </p>
          <label class="field">
            Clé API Ollama
            <input type="password" value={chat.webKey} placeholder="Colle ta clé ici" autocomplete="off" spellcheck="false"
              onchange={(e) => chat.setWebKey(e.currentTarget.value)} bind:this={keyInput} />
          </label>
          <div class="row">
            <button class="primary" disabled={!keyInput?.value.trim() && !chat.webKey} onclick={() => chat.connectCloud(keyInput?.value || chat.webKey)}>
              {chat.cloudStatus === "checking" ? "Test en cours…" : "Connecter et tester"}
            </button>
            {#if chat.webKey}<button onclick={() => chat.disconnectCloud()}>Déconnecter</button>{/if}
          </div>
          {#if chat.cloudStatus === "ok"}
            <p class="muted okline">Connecté. Ollama accepte ta clé et {chat.cloudModels.length} modèles sont disponibles.</p>
          {:else if chat.cloudStatus === "listed"}
            <p class="muted">{chat.cloudModels.length} modèles trouvés, mais la clé n'a pas encore été testée. Clique sur « Connecter et tester ».</p>
          {:else if chat.cloudStatus === "error"}
            <p class="muted ctxnote bad">{chat.cloudError}</p>
          {/if}
          <p class="muted">La clé est rangée dans le coffre Windows de ce profil. Elle n'est envoyée qu'à ollama.com.</p>
        </section>

      {:else if tab === "local"}
        <HardwarePanel {chat} />

      {:else if tab === "models"}
        <section>
          <h3>Mémoire de la conversation</h3>
          <p class="muted">
            Chaque modèle a une limite : ce qu'il peut garder en tête dans une même conversation (son contexte). Lama la lit chez Ollama et s'y adapte tout seul
            ({chat.modelMaxCtx ? `${formatTokens(chat.modelMaxCtx)} jetons pour ${chat.model}` : "limite inconnue pour le modèle actuel, 32 k par défaut"}).
            Quand la conversation arrive à {Math.round(COMPACT_AT * 100)} % de la limite, les vieux messages sont résumés pour faire de la place.
            Tu peux aussi le faire à la main avec le bouton jauge ou <code>/compact</code>.
          </p>
          {#if chat.unavailable[chat.model]}
            <p class="muted ctxnote bad">« {chat.model} » n'est plus proposé par Ollama Cloud (erreur 410). Choisis-en un autre.</p>
          {/if}
        </section>
        <section>
          <h3>Mode Plan</h3>
          <label class="field">
            Modèle utilisé pour réfléchir
            <Select
              value={chat.planModel}
              items={[{ value: "", label: "Le même que d'habitude" }, ...chat.models.map((m) => ({ value: m.name, label: m.label ?? m.name }))]}
              onchange={(v) => chat.setPlanModel(v)}
            />
          </label>
          <p class="muted">Un bon réflexe : un gros modèle pour planifier, puis un plus léger pour faire le travail.</p>
        </section>
        <section>
          <h3>Quel modèle pour quoi ?</h3>
          <ModelGuide {chat} />
        </section>

      {:else if tab === "web"}
        <section>
          <p class="muted okline" class:bad={!chat.online}>
            {chat.online ? "Tu es en ligne : la recherche web est active." : "Tu es hors ligne en ce moment : la recherche web est coupée."}
          </p>
          <p class="muted">
            Elle est toujours active, avec tous les modèles, cloud comme locaux. Elle se coupe toute seule quand ton ordinateur n'a plus internet, et revient quand il en retrouve.
          </p>
          <div class="row"><button onclick={() => chat.checkOnline(true)}>Vérifier la connexion</button></div>
          <h3>D'où viennent les résultats</h3>
          <p class="muted">
            Lama passe par DuckDuckGo, sans compte ni clé. DuckDuckGo n'a pas d'API officielle pour ça : s'il ne répond pas ou s'il bloque l'application,
            Lama se rabat sur Wikipédia et le dit au modèle. Les résultats sont alors moins larges, surtout pour l'actualité.
          </p>
          <p class="muted">En mode Manuel, Lama te demande ton accord avant de lire une page. Les adresses de ton réseau local sont toujours refusées.</p>
          <h3>Navigateur intégré</h3>
          <p class="muted">
            Un panneau à droite affiche la vraie page web. Il s'ouvre quand Lama cherche ou lit une page, ou depuis « Navigateur » dans la barre latérale.
            Il est en navigation privée (rien n'est gardé), refuse les téléchargements et n'a aucun accès à tes données ni à Lama : ce que tu y vois n'est pas envoyé au modèle.
          </p>
          <label class="check">
            <input type="checkbox" checked={chat.browserAuto} onchange={(e) => chat.setBrowserAuto(e.currentTarget.checked)} />
            <span>Ouvrir le navigateur quand Lama cherche, et y afficher les liens des réponses.</span>
          </label>
          <p class="muted">Astuce : Ctrl + clic sur un lien l'ouvre dans ton navigateur habituel.</p>
        </section>

      {:else if tab === "memory"}
        <section>
          <p class="muted">
            Lama envoie ces notes au modèle dans toutes tes conversations. Pour en ajouter, écris-les ici, tape <code>/remember</code> dans le chat ou demande-le à Lama.
            <code>/memoire</code> résume la conversation en cours en quelques lignes.
          </p>
          <p class="muted">Ce que Lama sait de toi (usages, style de réponse) se règle dans <button class="linkbtn" onclick={() => (tab = "profile")}>Mon profil</button>.</p>
          <form class="row" onsubmit={(e) => { e.preventDefault(); addMemory(); }}>
            <input class="grow" bind:value={newMemory} placeholder="Par exemple : je préfère les réponses courtes" aria-label="Nouveau souvenir" />
            <button class="primary" disabled={!newMemory.trim()}>Ajouter</button>
          </form>
          <ul>
            {#each chat.memories as m (m.id)}
              <li><span class="grow">{m.text}</span>
                <button class="del" aria-label="Oublier" onclick={() => chat.deleteMemory(m.id)}><Icon name="close" size={14} /></button></li>
            {:else}
              <li class="muted">Rien pour l'instant.</li>
            {/each}
          </ul>
        </section>

      {:else if tab === "profile"}
        {#key chat.onboardVersion}<MyProfilePanel {chat} defaultName={profileName} />{/key}
      {:else if tab === "progress"}
        <ProgressPanel {chat} />
      {:else if tab === "usage"}
        <UsagePanel {chat} />
      {:else if tab === "system"}
        <ComputerPanel />
      {:else if tab === "skills"}
        <SkillsPanel {chat} />
      {:else if tab === "agents"}
        <AgentsPanel {chat} />
      {:else if tab === "connectors"}
        <ConnectorsCatalog {servers} onreload={reload} />
        <section>
          <h3>Configuration avancée</h3>
          <p class="muted">
            Les connecteurs sont des serveurs MCP, avec le même format de fichier que Claude Desktop (<code>mcpServers</code>).
            Tu peux écrire le tien ici. Avant d'utiliser un de leurs outils, l'agent te demande ton accord, sauf en mode « Ignorer les permissions ».
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
      {:else if tab === "vault"}
        <VaultPanel />
      {:else if tab === "data"}
        <DataPanel {chat} />
      {:else if tab === "stats"}
        <AnalyticsPanel />
      {:else if tab === "update"}
        <UpdatePanel {chat} />
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
  .ph { display: grid; gap: 6px; padding-bottom: 6px; border-bottom: 1px solid var(--border); }
  .ph h2 { margin: 0; font-size: 22px; font-weight: 600; }
  .ph p { margin: 0; color: var(--muted); font-size: 14px; line-height: 1.5; max-width: 62ch; }
  .none { margin: 10px; color: var(--muted); font-size: 13px; }
  .muted.bad, .okline.bad { color: var(--del); }
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
