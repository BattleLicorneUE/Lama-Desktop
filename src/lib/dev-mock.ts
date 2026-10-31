/**
 * Simulation de l'API Tauri pour afficher l'interface dans un simple navigateur (`npm run dev`).
 * Chargé uniquement en développement et hors de la fenêtre Tauri : n'a aucun effet dans l'application.
 */

type Json = Record<string, any>;

export function installMock() {
  const w = window as any;
  if (w.__TAURI_INTERNALS__) return;

  const callbacks = new Map<number, (e: unknown) => void>();
  let localInstalled: { id: string; name: string; size_bytes: number; tools: boolean; vision: boolean; license: string; ctx: number; ctx_setting: number; auto_ctx: number; max_ctx: number; options: { ctx: number; mem_mb: number }[] }[] = [];
  let agentsMock: { name: string; description: string; prompt: string; model: string | null; tools: string[] | null; permission: string | null; skills: string[] | null; scope: string }[] = [
    { name: "explorateur", description: "Explore le code en lecture seule et résume.", prompt: "", model: null, tools: ["read_file", "list_dir", "grep"], permission: null, skills: null, scope: "builtin" },
    { name: "relecteur", description: "Relit du code : bugs, cas limites, sécurité.", prompt: "", model: null, tools: ["read_file", "list_dir", "grep"], permission: null, skills: null, scope: "builtin" },
    { name: "debogueur", description: "Trouve la cause racine d'un bug et corrige.", prompt: "", model: null, tools: null, permission: null, skills: null, scope: "builtin" },
  ];
  let localCancelled = false;
  let localPartial = 734003200;
  const listeners = new Map<string, number[]>();
  let nextId = 1;

  const emit = (event: string, payload: Json) =>
    (listeners.get(event) ?? []).forEach((id) => callbacks.get(id)?.({ event, id: 0, payload }));
  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const params = new URLSearchParams(location.search);
  const scenario = params.get("mock") ?? "ok"; // ok | offline | empty | error

  const models = [
    { name: "qwen3:8b", size: 5.2e9, modified_at: "" },
    { name: "llama3:latest", size: 4.7e9, modified_at: "" },
    { name: "qwen3-coder:480b-cloud", size: 382, modified_at: "", remote_host: "https://ollama.com:443" },
    { name: "gpt-oss:120b-cloud", size: 384, modified_at: "", remote_host: "https://ollama.com:443" },
  ];
  const now = Math.floor(Date.now() / 1000);
  const convs: Json[] = [
    { id: "c1", title: "Explique les lifetimes en Rust", model: "qwen3:8b", updated_at: now, kind: "chat", workspace: null, project_id: null },
    { id: "c2", title: "Recette de pâte à crêpes", model: "llama3:latest", updated_at: now - 3600, kind: "chat", workspace: null, project_id: "p1" },
    { id: "c3", title: "Corriger le site The Purifier", model: "qwen3:8b", updated_at: now - 7200, kind: "code", workspace: "C:\\Users\\Battl\\Desktop\\the-purifier", project_id: null },
  ];
  const artifactsMock: Json[] = [
    { id: "a1", title: "Glossaire C++ Unreal Engine 5", lang: "markdown", kind: "document", code: "# Glossaire C++ Unreal Engine 5\n\n## Comment utiliser ce glossaire\n\nCe glossaire couvre le vocabulaire nécessaire pour être autonome en C++ sous Unreal Engine 5.\n\n- **UObject** : classe de base.\n- **AActor** : objet placé dans le monde.\n", conv_id: "c1", created_at: now, updated_at: now - 86400 * 9, versions: 3 },
    { id: "a2", title: "Pitch The Purifier", lang: "html", kind: "slides", code: "<!doctype html><html><body style='margin:0;font-family:sans-serif;background:#111;color:#f4e9c8;display:grid;place-items:center;height:100vh'><section><h1 style='font-size:64px'>The Purifier</h1><p>Un jeu d'horreur psychologique</p></section></body></html>", conv_id: null, created_at: now, updated_at: now - 3600, versions: 1 },
  ];
  let customServers: string[] = [];
  const vm = { exists: false, open: false, master: "", failures: 0, agent: false, logins: [] as Json[], infos: [] as Json[] };
  let projectMem: Json[] = [{ id: 901, text: "Public : familles, recettes simples", project_id: "p1" }];
  let tasksMock: Json[] = [
    { id: "t1", name: "Revue de presse du matin", prompt: "Cherche les nouveautés sur Tauri et résume-les en 5 points.", kind: "chat", workspace: null, project_id: null, repeat: "daily", time: "08:00", weekday: 0, date: null, enabled: true, next_at: Math.floor(Date.now() / 1000) + 86400, last_at: Math.floor(Date.now() / 1000) - 86400, last_conv: null },
  ];
  let projects: Json[] = [
    { id: "p1", name: "Cuisine", instructions: "Réponds comme un chef. Propose toujours une variante végétarienne.", kind: "chat", workspace: null, pinned: true, updated_at: now - 3600, chats: 1 },
    { id: "p2", name: "ILOI", instructions: "L'intuition de l'image de l'océan Indien", kind: "chat", workspace: null, pinned: false, updated_at: now - 86400 * 30, chats: 0 },
    { id: "p3", name: "The Purifier", instructions: "The Purifier est un jeu vidéo d'aventure psychologique et horrifique à la première personne, développé sous Unreal Engine 5, se déroulant dans une France médiévale fictive.", kind: "chat", workspace: null, pinned: false, updated_at: now - 86400 * 400, chats: 0 },
    { id: "p4", name: "the-purifier", instructions: "Projet Unreal Engine 5. Compiler avec UnrealBuildTool avant de conclure.", kind: "code", workspace: "C:/Users/Battl/Desktop/the-purifier", pinned: false, updated_at: now - 7200, chats: 1 },
  ];
  let skills: { name: string; description: string; dir: string; scope: string; enabled: boolean; argument_hint?: string | null; model_invocable?: boolean; user_invocable?: boolean }[] = [
    { name: "frontend-design", description: "Crée des interfaces web soignées, loin des gabarits génériques.", dir: "C:\\skills\\frontend-design", scope: "app", enabled: true },
    { name: "revue-de-code", description: "Relit un diff et signale bugs et oublis.", dir: "C:\\skills\\revue", scope: "app", enabled: true },
    { name: "pdf", description: "Lire et remplir des PDF.", dir: "C:\\skills\\pdf", scope: "app", enabled: false },
  ];

  const reply =
    "Voici un exemple de **réponse** avec du code :\n\n```rust\nfn main() {\n    println!(\"Bonjour\");\n}\n```\n\n- un point\n- un autre point\n\nEt une formule : $E = mc^2$.";

  async function stream(id: string, text: string) {
    await wait(300);
    for (const part of text.match(/.{1,12}/gs) ?? []) {
      emit("chat:chunk", { id, chunk: { content: part, thinking: "", tool_calls: [], done: false } });
      await wait(25);
    }
    emit("chat:usage", { id, prompt_tokens: 2150 });
    emit("chat:chunk", { id, chunk: { content: "", thinking: "", tool_calls: [], done: true } });
  }

  let approval: ((ok: boolean) => void) | null = null;

  async function agent(id: string) {
    const tool = (call_id: string, name: string, args: Json, state: string, output?: string, diff?: string) =>
      emit("agent:tool", { id, call_id, name, args, state, output: output ?? null, diff: diff ?? null });
    await wait(300);
    emit("agent:tasks", {
      id,
      tasks: [
        { content: "Lire la page d'accueil", status: "completed" },
        { content: "Refaire le bandeau et la palette", status: "in_progress" },
        { content: "Vérifier avec les tests", status: "pending" },
      ],
    });
    const say = (content: string) => emit("chat:chunk", { id, chunk: { content, thinking: "", tool_calls: [], done: false } });
    say("Je lis d'abord la page d'accueil.");
    await wait(200);
    emit("agent:step", { id });
    for (let i = 0; i < 3; i++) tool("n" + i, "mcp__notion__API-post-search", { query: "tâches " + i }, "done", "{}");
    tool("n9", "mcp__notion__API-retrieve-a-page", { page_id: "x" }, "error", "404");
    say("Notion répond, mais le contenu des bases est bloqué : je corrige le style en attendant.");
    await wait(200);
    emit("agent:step", { id });
    tool("t1", "read_file", { path: "index.html" }, "done", "    1\t<!doctype html>\n    2\t<title>The Purifier</title>");
    tool("w1", "web_search", { query: "dégradé radial CSS" }, "running");
    await wait(300);
    tool("w1", "web_search", { query: "dégradé radial CSS" }, "done", "1. MDN : radial-gradient()\n   https://developer.mozilla.org/fr/docs/Web/CSS/gradient/radial-gradient");
    await wait(400);
    const diff = "--- a/style.css\n+++ b/style.css\n@@ -1,3 +1,4 @@\n body {\n-  background: #777;\n+  background: radial-gradient(#1a1a2e, #0b0b14);\n+  color: #f4e9c8;\n }";
    tool("t2", "edit_file", { path: "style.css" }, "awaiting", null as any, diff);
    const ok = await new Promise<boolean>((r) => (approval = r));
    if (!ok) {
      tool("t2", "edit_file", { path: "style.css" }, "denied", null as any, diff);
    } else {
      tool("t2", "edit_file", { path: "style.css" }, "done", "Fichier écrit : style.css", diff);
      await wait(300);
      tool("t3", "verify", { target: "check" }, "running", "Vérification…");
      await wait(700);
      tool("t3", "verify", { target: "check" }, "done", "Aucune erreur.\n[code de sortie : 0]");
    }
    await stream(id, "J'ai remplacé le fond gris par un dégradé sombre et vérifié le projet : tout passe.");
  }

  let analyticsMock = { asked: false, enabled: false };
  let pc = { enabled: true, dirs: ["C:/Users/demo/Documents"] as string[] };
  let profiles = [
    { id: "p1", name: "Principal", color: "#c96442", legacy: true, locked: false },
    { id: "p2", name: "Léa", color: "#3a78e0", legacy: false, locked: true },
  ];

  async function handler(cmd: string, args: Json): Promise<unknown> {
    switch (cmd) {
      case "plugin:event|listen": {
        const list = listeners.get(args.event) ?? [];
        list.push(args.handler);
        listeners.set(args.event, list);
        return args.handler;
      }
      case "plugin:event|unlisten":
        return null;
      case "model_show":
        if (args.name === "qwen3-coder:480b-cloud") throw "Ollama a répondu 410 : Non disponible";
        if (args.name === "gpt-oss:120b")
          return { capabilities: ["completion", "tools", "thinking"], model_info: { "general.architecture": "gptoss", "gptoss.context_length": 131072 } };
        return {
          capabilities: args.name.includes("llama3") ? ["completion"] : ["completion", "tools"],
          details: { family: "qwen3", parameter_size: "8B", quantization_level: "Q4_K_M" },
          model_info: {
            "general.architecture": "qwen3",
            "qwen3.context_length": 40960,
            "qwen3.block_count": 36,
            "qwen3.attention.head_count": 32,
            "qwen3.attention.head_count_kv": 8,
            "qwen3.attention.key_length": 128,
            "qwen3.embedding_length": 4096,
          },
        };
      case "cloud_models":
        return [
          { name: "gpt-oss:120b", size: 65290180781, modified_at: "" },
          { name: "gemma4:31b", size: 62546177752, modified_at: "" },
          { name: "gpt-oss:20b", size: 13780162412, modified_at: "" },
        ];
      case "cloud_test":
        if (!args.key || args.key === "mauvaise") throw "Ollama a répondu 401 : clé API refusée par Ollama Cloud";
        return "ok";
      case "conv_list":
        return convs.filter((c) => c.kind === args.kind && (!args.projectId || c.project_id === args.projectId));
      case "conv_messages":
        return [
          { role: "user", content: "Explique les lifetimes en Rust", thinking: null, extra: null },
          { role: "assistant", content: reply, thinking: null, extra: null },
        ];
      case "conv_create":
        return { id: "new-" + Date.now(), title: "Nouvelle conversation", model: args.model, updated_at: now, kind: args.kind, workspace: args.workspace, project_id: args.projectId };
      case "project_list":
        return projects;
      case "artifact_list":
        return artifactsMock;
      case "artifact_save":
      case "artifact_delete":
        return null;
      case "project_pin":
        projects = projects.map((p) => (p.id === args.id ? { ...p, pinned: args.pinned } : p));
        return null;
      case "project_docs":
        return args.projectId === "p1"
          ? [
              { id: "d1", name: "carnet-recettes.md", content: "Pâte à crêpes : 250 g de farine, 3 œufs, 50 cl de lait. ".repeat(120) },
              { id: "d2", name: "allergies.txt", content: "Pas de noix, pas de cacahuètes." },
            ]
          : [];
      case "memory_add": {
        const m = { id: Date.now() + Math.floor(Math.random() * 1000), text: args.text, project_id: args.projectId ?? null };
        if (args.projectId) projectMem.push(m);
        return m;
      }
      case "memory_project_list":
        return projectMem.filter((m) => m.project_id === args.projectId);
      case "memory_delete":
        projectMem = projectMem.filter((m) => m.id !== args.id);
        return null;
      case "browser_status":
        return { available: true, name: "Brave", running: false };
      case "browser_launch_ui":
        return "Page ouverte : « DuckDuckGo » (https://duckduckgo.com/)";
      case "browser_close_ui":
        return "Navigateur fermé.";
      case "browser_wipe":
        return null;
      case "vault_browser_login":
        return "« GitHub » : identifiant et mot de passe saisi(s), Entrée pressée. Page maintenant : « GitHub » (https://github.com/).";
      case "vault_status":
        return { exists: vm.exists, unlocked: vm.open, agent_access: vm.agent, logins: vm.logins.length, infos: vm.infos.length, locks_in: vm.open ? 290 : 0, failures: vm.failures };
      case "vault_create":
        if (String(args.master).length < 10) throw "Le mot de passe maître doit faire au moins 10 caractères.";
        Object.assign(vm, { exists: true, open: true, master: args.master });
        return null;
      case "vault_unlock":
        if (args.master !== vm.master) {
          vm.failures++;
          throw "Mot de passe maître incorrect (ou coffre altéré).";
        }
        Object.assign(vm, { open: true, failures: 0 });
        return null;
      case "vault_lock":
        vm.open = false;
        return null;
      case "vault_list":
        return { logins: vm.logins.map(({ password: _p, notes: _n, ...l }) => l), infos: vm.infos };
      case "vault_login_get":
        return vm.logins.find((l) => l.id === args.id);
      case "vault_login_save": {
        const l = { ...args.login, id: args.login.id || "l" + (vm.logins.length + 1), updated: Math.floor(Date.now() / 1000) };
        vm.logins = vm.logins.some((x) => x.id === l.id) ? vm.logins.map((x) => (x.id === l.id ? l : x)) : [...vm.logins, l];
        return l;
      }
      case "vault_login_delete":
        vm.logins = vm.logins.filter((l) => l.id !== args.id);
        return null;
      case "vault_info_save": {
        const i = { ...args.info, id: args.info.id || "i" + (vm.infos.length + 1) };
        vm.infos = vm.infos.some((x) => x.id === i.id) ? vm.infos.map((x) => (x.id === i.id ? i : x)) : [...vm.infos, i];
        return i;
      }
      case "vault_info_delete":
        vm.infos = vm.infos.filter((i) => i.id !== args.id);
        return null;
      case "vault_generate": {
        const o = args.opts;
        const pool = (o.lower ? "abcdefghjkmnpqrstuvwxyz" : "") + (o.upper ? "ABCDEFGHJKLMNPQRSTUVWXYZ" : "") + (o.digits ? "23456789" : "") + (o.symbols ? "!@#$%^&*-_=+?." : "");
        if (!pool) throw "Coche au moins un type de caractères.";
        let p = "";
        for (let k = 0; k < o.length; k++) p += pool[Math.floor(Math.random() * pool.length)];
        return { password: p, bits: o.length * Math.log2(pool.length) };
      }
      case "vault_set_agent_access":
        vm.agent = args.on;
        return null;
      case "vault_change_master":
        if (args.old !== vm.master) throw "L'ancien mot de passe maître est incorrect.";
        vm.master = args.new;
        return null;
      case "vault_destroy":
        if (args.master !== vm.master) throw "Mot de passe maître incorrect.";
        Object.assign(vm, { exists: false, open: false, logins: [], infos: [] });
        return null;
      case "vault_copy":
        return 30;
      case "vault_fill_ui":
        return "identifiant et mot de passe de « GitHub » : saisi dans la fenêtre « Connexion — Navigateur ».";
      case "schedules_list":
        return tasksMock;
      case "schedule_save": {
        const t = { ...args.task, id: args.task.id || "t" + Date.now() };
        tasksMock = tasksMock.some((x) => x.id === t.id) ? tasksMock.map((x) => (x.id === t.id ? t : x)) : [...tasksMock, t];
        return t;
      }
      case "schedule_delete":
        tasksMock = tasksMock.filter((t) => t.id !== args.id);
        return null;
      case "schedule_done":
        tasksMock = tasksMock.map((t) => (t.id === args.id ? { ...t, next_at: args.nextAt, last_conv: args.conv ?? t.last_conv, last_at: Math.floor(Date.now() / 1000) } : t));
        return null;
      case "data_backup":
        return 7;
      case "data_restore":
        return 7;
      case "data_export_chats":
        return 3;
      case "data_erase":
        throw "Une génération est en cours : arrête-la avant d'effacer.";
      case "plugin:dialog|save":
        return "C:\Users\Battl\Documents\Lama-sauvegarde.zip";
      case "memory_list":
        return [{ id: 1, text: "Préfère les réponses courtes et en français" }];
      case "skills_list":
        return skills;
      case "skill_get": {
        const s = skills.find((x) => x.name === args.name);
        if (!s) throw "Skill introuvable.";
        return { draft: { name: s.name, description: s.description, body: "# Objectif\n\nInstructions de " + s.name + ".", argument_hint: s.argument_hint ?? "", model_invocable: s.model_invocable !== false }, scope: s.scope, dir: s.dir };
      }
      case "skill_save": {
        const d = args.draft as { name: string; description: string; argument_hint: string; model_invocable: boolean };
        const slug = d.name.trim().toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "");
        if (skills.some((x) => x.name === slug && x.name !== args.original)) throw `Un skill « ${slug} » existe déjà : choisis un autre nom, ou modifie celui-là.`;
        const entry = { name: slug, description: d.description, dir: "C:/skills/" + slug, scope: String(args.scope), enabled: true, argument_hint: d.argument_hint || null, model_invocable: d.model_invocable, user_invocable: true };
        skills = args.original ? skills.map((x) => (x.name === args.original ? entry : x)) : [...skills, entry];
        return entry.dir;
      }
      case "skill_delete":
        skills = skills.filter((x) => x.name !== args.name);
        return null;
      case "skills_hub_browse":
        return [
          { rank: 1, name: "find-skills", source: "vercel-labs/skills", id: "vercel-labs/skills/find-skills", installs: "3.9M" },
          { rank: 2, name: "frontend-design", source: "anthropics/skills", id: "anthropics/skills/frontend-design", installs: "1.2M" },
        ];
      case "skills_auto_import":
        return 0;
      case "git_status":
        return { repo: true, branch: "main", files: [{ path: "src/app.js", status: "M", staged: false }, { path: "notes.md", status: "?", staged: false }] };
      case "git_diff":
        return ["diff --git a/src/app.js b/src/app.js", "@@ -1,2 +1,2 @@", "-const a = 1;", "+const a = 2;", " export default a;"].join(String.fromCharCode(10));
      case "git_commit":
        return "[main 1a2b3c4] mock";
      case "reminders_list":
        return [{ id: "a1b2c3d4", title: "Gratin", body: "Sors le gratin du four", at: Math.floor(Date.now() / 1000) + 1200 }];
      case "reminders_cancel":
        return true;
      case "agent_changes":
        return [
          { path: "C:/demo/app.js", kind: "modified", diff: "--- a/app.js\n+++ b/app.js\n@@ -1,2 +1,2 @@\n-const a = 1;\n+const a = 2;\n let b = 3;\n" },
          { path: "C:/demo/notes.md", kind: "created", diff: "--- a/notes.md\n+++ b/notes.md\n@@ -0,0 +1 @@\n+Bonjour\n" },
        ];
      case "project_rules_get":
        return "# Règles\n- Lancer npm test après chaque modification.";
      case "project_rules_set":
      case "git_branch_create":
        return args.name ?? null;
      case "project_templates":
        return [
          { id: "web", label: "Page web", desc: "index.html, style.css, app.js : aucune installation.", prompt: "Construis une page web : " },
          { id: "python", label: "Script Python", desc: "main.py, test pytest, requirements.", prompt: "Écris un programme Python qui " },
        ];
      case "save_bytes":
      case "reveal_file":
        return null;
      case "profile_active":
        return profiles[0]?.id ?? null;
      case "window_new":
        return "win-demo";
      case "analytics_get":
        return { available: true, ...analyticsMock, events: [["app_started", "Lancement de l'application, avec la puissance du PC en gros (mémoire, type de carte graphique)."], ["chat_started", "Un message envoyé : cloud ou local, et le nom du modèle. Jamais le texte."], ["agent_started", "Une tâche confiée à l'agent : le mode choisi et le nom du modèle. Jamais le contenu."], ["local_model_installed", "Un modèle local installé : son nom dans le catalogue."], ["local_model_failed", "Une installation de modèle local qui a échoué : son nom dans le catalogue."], ["connector_added", "Un connecteur ajouté depuis le catalogue : son nom dans le catalogue."], ["local_engine_started", "Le moteur local a démarré : le modèle, la taille de contexte et le temps de démarrage."], ["local_engine_failed", "Le moteur local n'a pas pu démarrer : le modèle."], ["chat_error", "Une réponse qui a échoué : seulement le type d'erreur (clé refusée, limite atteinte, réseau…), jamais le message."], ["feature_used", "Une fonction essayée (sauvegarde, tâche planifiée, coffre, nouvelle fenêtre…) : son nom, jamais son contenu."], ["settings_page", "Une page des Réglages ouverte : son nom."], ["update_installed", "Une mise à jour installée : la version précédente."], ["app_closed", "La fermeture de l'application : la durée d'utilisation en minutes."], ["app_panic", "Un plantage interne : le fichier et la ligne du code concerné, jamais de données."]],
          sent: analyticsMock.enabled ? [{ t: Math.floor(Date.now() / 1000), name: "app_started", props: { ram_go: 24, gpu: "nvidia", vram_go: 8, avx2: true } }] : [] };
      case "analytics_set":
        analyticsMock = { asked: true, enabled: Boolean(args.enabled) };
        return handler("analytics_get", {});
      case "app_settings_get":
        return { keep_background: false, autostart: false, shortcut: "Ctrl+Alt+S", shortcut_ok: true };
      case "app_settings_set":
        return { keep_background: Boolean(args.keepBackground), autostart: Boolean(args.autostart), shortcut: "Ctrl+Alt+S", shortcut_ok: true };
      case "journal_list":
        return [
          { t: Math.floor(Date.now() / 1000) - 60, tool: "computer", summary: "click en (640, 360)", outcome: "done" },
          { t: Math.floor(Date.now() / 1000) - 300, tool: "run_command", summary: "npm test", outcome: "denied" },
        ];
      case "journal_clear":
        return null;
      case "last_shot":
        return null;
      case "git_review_diff":
        return "diff --git a/src/app.js b/src/app.js\n--- a/src/app.js\n+++ b/src/app.js\n@@ -1,4 +1,6 @@\n function total(items) {\n-  return items.reduce((a, b) => a + b.price, 0);\n+  let t = 0;\n+  for (let i = 0; i <= items.length; i++) t += items[i].price;\n+  return t;\n }\n";
      case "board_report":
        return null;
      case "board_focus":
        return null;
      case "window_task":
        return null;
      case "board_list":
        return [
          { label: "main", title: "Corriger le site The Purifier", workspace: "C:/demo/the-purifier", model: "gpt-oss:120b", state: "running", request_id: "r1", done: 1, total: 3, last: "edit_file style.css" },
          { label: "win-a1b2c3d4", title: "Tests du module de facturation", workspace: "C:/demo/factures", model: "qwen3-coder:480b", state: "waiting", request_id: "r2", done: 0, total: 4, last: "run_command npm test" },
          { label: "win-e5f6a7b8", title: "Recherche sur les batteries solides", workspace: "", model: "gpt-oss:120b", state: "done", request_id: null, done: 0, total: 0, last: "" },
        ];
      case "connectors_catalog":
        return [
          { id: "fetch", name: "Pages web", desc: "Lire n'importe quelle page web et la convertir en texte.", category: "Essentiels", needs: "uv", command: "uvx", fields: [], setup: "", docs: "" },
        ];
      case "connectors_installed":
        return ["fetch", ...customServers];
      case "mcp_add_custom":
        customServers.push(String(args.name).toLowerCase().replace(/[^a-z0-9]+/g, "-"));
        return customServers.at(-1);
      case "mcp_import":
        customServers.push("importe");
        return ["importe"];
      case "mcp_remove":
        customServers = customServers.filter((n) => n !== args.name);
        return null;
      case "connector_add":
      case "connector_remove":
        return null;
      case "hardware_detect":
        return { ram_mb: 24260, ram_free_mb: 14811, cpu: "AMD Ryzen 7 7435HS", cores: 16, avx2: true, disk_free_mb: 69919, gpus: [{ name: "NVIDIA GeForce RTX 4060 Laptop GPU", vendor: "nvidia", vram_mb: 8188, integrated: false }] };
      case "net_online":
        return true;
      case "local_installed":
        return localInstalled;
      case "local_status":
        return { running: null, installing: null, engine_bytes: localInstalled.length ? 38000000 : 0, partial_bytes: localPartial, free_mb: 69919 - localInstalled.length * 5000 };
      case "local_set_ctx":
        localInstalled = localInstalled.map((m) => (m.id === args.id ? { ...m, ctx_setting: Number(args.ctx), ctx: Number(args.ctx) || m.auto_ctx } : m));
        return null;
      case "local_cancel":
        localCancelled = true;
        return null;
      case "local_remove": {
        const gone = localInstalled.find((m) => m.id === args.id);
        localInstalled = localInstalled.filter((m) => m.id !== args.id);
        return gone?.size_bytes ?? 0;
      }
      case "local_clean_partials": {
        const freed = localPartial;
        localPartial = 0;
        return freed;
      }
      case "local_install": {
        const id = String(args.id);
        localCancelled = false;
        void (async () => {
          const total = id === "qwen3-14b" ? 9000000000 : 5027783488;
          const label = id === "qwen3-14b" ? "Qwen3 14B" : "Qwen3 8B";
          for (let step = 1; step <= 10 && !localCancelled; step++) {
            emit("local:progress", { id, phase: step < 2 ? "moteur" : "modèle", file: id + ".gguf", done: (total / 10) * step, total });
            await new Promise((r) => setTimeout(r, 600));
          }
          if (localCancelled) return emit("local:error", { id, error: "Installation annulée." });
          localInstalled = [...localInstalled, { id, name: label, size_bytes: total, tools: true, vision: false, license: "Apache-2.0", ctx: 32768, ctx_setting: 0, auto_ctx: 32768, max_ctx: 32768, options: [{ ctx: 8192, mem_mb: 1152 }, { ctx: 16384, mem_mb: 1250 }, { ctx: 32768, mem_mb: 2442 }] }];
          emit("local:done", { id });
        })();
        return null;
      }
      case "local_models_assess":
        return [
          { id: "qwen3-8b", name: "Qwen3 8B", params_b: 8.2, size_mb: 5000, uses: ["chat", "reasoning", "code"], tools: true, vision: false, blurb: "Le bon choix général dès qu'on a une carte graphique de 8 Go.", level: "recommended", mode: "gpu", speed: "fast", reason: "Tient entièrement dans ta carte graphique (8,0 Go) : rapide.", disk_ok: true, need_mb: 6500, best: true },
          { id: "qwen3-14b", name: "Qwen3 14B", params_b: 14.8, size_mb: 9000, uses: ["chat", "reasoning"], tools: true, vision: false, blurb: "Très bon raisonnement.", level: "possible", mode: "mixed", speed: "medium", reason: "Il faut 10,3 Go : une partie dans la carte graphique, le reste dans la mémoire du PC. Vitesse correcte.", disk_ok: true, need_mb: 10500, best: false },
          { id: "gemma3-4b", name: "Gemma 3 4B", params_b: 4.3, size_mb: 3300, uses: ["chat"], tools: false, vision: true, blurb: "Comprend les images.", level: "heavy", mode: null, speed: null, reason: "Exemple de modèle trop lourd.", disk_ok: true, need_mb: 4800, best: false },
        ];
      case "pc_get":
        return pc;
      case "pc_set":
        pc = { enabled: Boolean(args.enabled), dirs: (args.dirs as string[]) ?? [] };
        return pc;
      case "bg_list":
        return [{ id: "a1b2c3d4", command: "npm run dev", running: true, code: null, started_ms: Date.now(), tail: "VITE ready in 312 ms" }];
      case "bg_stop":
      case "bg_clear":
        return null;
      case "chat_once":
        if (String((args.messages as { content: string }[])?.[0]?.content ?? "").includes("tâche planifiée"))
          return "```json\n" + JSON.stringify({ name: "Revue de presse Tauri", prompt: "Cherche sur le web les nouveautés sur Tauri de la semaine et résume-les en 5 points, chacun suivi de sa source." }) + "\n```";
        if (String((args.messages as { content: string }[])?.[0]?.content ?? "").includes("instructions d'un projet"))
          return JSON.stringify({ name: "Mémoire d'histoire", instructions: "Tu aides à rédiger un mémoire de master en histoire.\nTon : sobre et précis.\nCite toujours tes sources." });
        if (String((args.messages as { content: string }[])?.[0]?.content ?? "").includes("« skills »"))
          return "Voici le skill :\n```json\n" + JSON.stringify({ name: "repondre-aux-clients", description: "Utilise ce skill quand on demande de répondre à un mail de client, poliment et en deux phrases.", argument_hint: "[le mail du client]", body: "# Réponse client\n\nMail reçu : $ARGUMENTS\n\n1. Remercie le client.\n2. Réponds à sa question en deux phrases.\n3. Termine par une formule polie.\n" }) + "\n```";
        if (String((args.messages as { content: string }[])?.[0]?.content ?? "").includes("relecteur de code senior"))
          return JSON.stringify({ summary: "La fonction total() a été réécrite avec une boucle.", findings: [
            { file: "src/app.js", line: 3, severity: "bug", title: "Boucle qui dépasse le tableau", detail: "La condition i <= items.length lit items[items.length], qui est undefined : TypeError sur la dernière itération.", fix: "Utiliser i < items.length (ou garder reduce)." },
            { file: "src/app.js", line: 2, severity: "style", title: "Variable t peu lisible", detail: "Un nom explicite (somme) rendrait le code plus clair.", fix: "Renommer t en somme." },
          ] });
        return ["TITRE: Lifetimes en Rust", "- L'utilisateur apprend les lifetimes en Rust", "- Exemple retenu : une fonction avec une référence", "- Prochaine étape : étudier le borrow checker"].join(String.fromCharCode(10));
      case "save_text":
        return null;
      case "secret_get":
        return localStorage.getItem("mock.secret");
      case "secret_set":
        if (args.value) localStorage.setItem("mock.secret", args.value);
        else localStorage.removeItem("mock.secret");
        return null;
      case "profiles_list":
        return location.search.includes("fresh") && !profiles.some((p) => p.id.startsWith("n")) ? [] : profiles;
      case "profile_create": {
        const p = { id: (location.search.includes("fresh") ? "n" : "p") + (profiles.length + 1), name: args.name, color: args.color, legacy: false, locked: false };
        profiles.push(p);
        return p;
      }
      case "profile_update": {
        const p = profiles.find((x) => x.id === args.id);
        if (p) Object.assign(p, { name: args.name, color: args.color });
        return p;
      }
      case "profile_set_pin": {
        const p = profiles.find((x) => x.id === args.id);
        if (p) (p as { locked?: boolean }).locked = !!args.newPin;
        return null;
      }
      case "profile_delete":
        profiles = profiles.filter((x) => x.id !== args.id);
        return null;
      case "profile_select": {
        const p = profiles.find((x) => x.id === args.id) as { locked?: boolean } | undefined;
        if (p?.locked && args.pin !== "1234") throw "Code PIN incorrect.";
        return null;
      }
      case "agent_get": {
        const a = agentsMock.find((x) => x.name === args.name);
        if (!a) throw "Agent introuvable.";
        return { draft: { name: a.name, description: a.description, prompt: a.prompt || "Tu es un agent de démonstration. Réponds par une liste courte.", model: a.model ?? "", tools: a.tools }, scope: a.scope };
      }
      case "agent_save": {
        const d = args.draft as { name: string; description: string; prompt: string; model: string; tools: string[] | null };
        const slug = d.name.trim().toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "");
        if (agentsMock.some((x) => x.name === slug && x.name !== args.original)) throw `Un agent « ${slug} » existe déjà : choisis un autre nom, ou modifie celui-là.`;
        const entry = { name: slug, description: d.description, prompt: d.prompt, model: d.model || null, tools: d.tools, permission: null, skills: null, scope: String(args.scope) };
        agentsMock = args.original ? agentsMock.map((x) => (x.name === args.original ? entry : x)) : [...agentsMock, entry];
        return "C:/agents/" + slug + ".md";
      }
      case "agent_delete":
        agentsMock = agentsMock.filter((x) => x.name !== args.name);
        return null;
      case "agents_list":
        return agentsMock;
      case "agents_import_claude":
        return 2;
      case "agents_open_dir":
        return null;
      case "context_report":
        return { system: 8800 * 3.5, skills: 10000 * 3.5, tools: 20700 * 3.5, mcp: 17900 * 3.5 };
      case "mcp_reload":
      case "mcp_status":
        return [{ name: "fetch", tools: ["fetch"], error: null }, ...customServers.map((name) => ({ name, tools: ["a", "b"], error: null }))];
      case "chat_start":
        if (scenario === "error") {
          setTimeout(() => emit("chat:error", { id: args.requestId, error: "Ollama a répondu 500 : model failed to load" }), 400);
        } else void stream(args.requestId, reply);
        return null;
      case "agent_start":
        void agent(args.requestId);
        return null;
      case "agent_approve":
        approval?.(args.allow);
        return null;
      case "agent_inject":
        return true;
      case "chat_cancel":
      case "conv_save":
      case "conv_rename":
      case "conv_delete":
        return null;
      case "fs_create":
      case "fs_rename":
      case "fs_delete":
        return null;
      case "fs_list":
        return args.rel && args.rel !== "." ? [{ name: "main.js", is_dir: false }] : [{ name: "assets", is_dir: true }, { name: "index.html", is_dir: false }, { name: "style.css", is_dir: false }];
      case "plugin:dialog|open":
        return null;
      default:
        return null;
    }
  }

  w.__TAURI_INTERNALS__ = {
    transformCallback(cb: (e: unknown) => void) {
      const id = nextId++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
    invoke: (cmd: string, args: Json = {}) => handler(cmd, args),
    convertFileSrc: (p: string) => p,
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
}
