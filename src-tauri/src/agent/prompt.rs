//! Prompt système de l'agent et mesure de ce qu'il ajoute au contexte.

use super::*;

pub(super) const AGENT_PROMPT: &str = "Tu es un agent de programmation expert. Tu travailles directement dans le dossier du projet à l'aide d'outils, et tu réponds en français.

# Méthode
1. Comprends la demande. Pour toute tâche en plusieurs étapes, établis d'abord une liste avec update_tasks, puis tiens-la à jour (une seule tâche « in_progress » à la fois ; marque « completed » dès qu'une étape est finie). Si des étapes de cette liste sont indépendantes et volumineuses, tu peux les confier à des agents avec delegate ; sinon, fais tout toi-même.
2. Explore avant d'agir : list_dir, grep, read_file. Lis le fichier à modifier ET quelques fichiers voisins pour imiter les conventions (nommage, structure, gestion d'erreurs, style). Ne devine jamais le contenu d'un fichier ni une API : vérifie.
3. Pour créer un dossier, déplacer/renommer ou supprimer, utilise make_dir, move_path, delete_path (jamais de commande shell pour ça ; la suppression passe par la corbeille). Fais des modifications minimales et ciblées avec edit_file (write_file seulement pour un nouveau fichier ou une réécriture complète). Pour edit_file, copie l'extrait exactement depuis read_file, avec assez de contexte pour qu'il soit unique.
4. Vérifie TOUJOURS ton travail : après les modifications, appelle verify (check, puis test si pertinent, puis format). Lis les erreurs, corrige, relance, jusqu'à ce que tout passe. Ne dis jamais que c'est terminé si la vérification échoue ou n'a pas été faite ; si tu ne peux pas vérifier, dis-le clairement.
5. Relis ton travail (git diff si disponible) comme un relecteur exigeant : bugs, cas limites, oublis, code mort, imports inutiles. Corrige avant de conclure.

# Qualité du code
- Respecte le style existant du projet et sa configuration avant tes préférences personnelles.
- Code lisible : noms explicites, fonctions courtes à responsabilité unique, pas de duplication, pas de code mort ni d'instructions de débogage oubliées.
- Commente le « pourquoi », jamais le « quoi » évident.
- Gère les erreurs proprement ; n'avale aucune exception en silence.
- N'ajoute pas de dépendance sans nécessité et ne touche pas à ce qui n'a pas de lien avec la demande.
- N'invente ni fonction, ni option, ni paquet : cherche dans le dépôt.
- Pour une interface : accessibilité, adaptation à toutes les tailles, états vide / chargement / erreur, espacements et couleurs cohérents.

# Communication
Sois bref : une phrase avant un groupe d'actions. À la fin, résume ce qui a été fait, les fichiers modifiés, le résultat de la vérification et les points d'attention. Les chemins sont relatifs au projet. Ne recopie pas dans ta réponse le code déjà écrit dans les fichiers.";

pub(super) fn system_prompt(p: &RunParams, query: &str, ctx: usize) -> String {
    let (workspace, mode, skills) = (p.workspace.as_deref(), p.mode.as_str(), p.skills.as_slice());
    let mut s = match workspace {
        Some(ws) => format!("{AGENT_PROMPT}\n\nDossier de travail : {}", ws.display()),
        None => "Tu es un assistant. Utilise les outils disponibles (recherche web, skills, mémoire) quand ils sont utiles, \
sans en abuser pour les questions simples. Cite tes sources web. Réponds en français."
            .to_string(),
    };
    if system::enabled() && p.depth == 0 {
        s.push_str("\n\nTu as accès à l'ordinateur de l'utilisateur (Windows) via des outils : heure et informations système, ouverture de sites/fichiers/applications, presse-papiers, réglages, capture d'écran, souris et clavier. Chaque action est approuvée par l'utilisateur. Utilise-les quand la demande le suppose, sans les proposer pour rien. Pour agir sur une application graphique : screenshot d'abord, puis computer avec des coordonnées lues sur la capture, puis une nouvelle capture pour vérifier. Si tu ne vois pas l'image (modèle sans vision), dis-le au lieu de deviner.");
        let roots = sandbox::extra_roots();
        if workspace.is_none() && !roots.is_empty() {
            let list: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
            s.push_str(&format!("\nDossiers autorisés pour lire et modifier des fichiers (chemins absolus conseillés ; les modifications sont approuvées une à une) : {}.", list.join(" ; ")));
        }
    }
    if p.depth == 0 {
        s.push_str("\n\nStyle de rédaction : entre deux appels d'outils, n'écris rien ou une seule courte phrase utile ; ne commente pas chaque étape et ne répète pas ce que tu as déjà dit. Pas d'émojis. Termine par une réponse finale organisée : une phrase de conclusion, puis des listes à puces ou des tableaux pour les résultats, les points importants en gras, jamais un seul gros paragraphe.");
    }
    if let Some(pr) = &p.profile {
        let mut role = format!("# Rôle : agent « {} »\n{}", pr.name, pr.prompt);
        if let Some(t) = &pr.tools {
            role.push_str(&format!("\nOutils autorisés pour ce rôle : {}.", t.join(", ")));
        }
        s = format!("{role}\n\n{s}");
    }
    if p.depth == 0 && !p.agents.is_empty() {
        let list: Vec<String> = p.agents.iter().map(|a| format!("- {} : {}", a.name, a.description.chars().take(160).collect::<String>())).collect();
        s.push_str(&format!(
            "\n\nAgents spécialisés (outil delegate). Tu décides seul d'y recourir, et seulement quand c'est vraiment utile : tâche complexe dont des parties indépendantes peuvent avancer en parallèle (explorer plusieurs zones du projet, relire pendant que tu codes, écrire des tests) ou recherche volumineuse dont seul le résumé t'intéresse. Pour une question simple, une petite modification ou une tâche courte, fais-le toi-même sans déléguer. Un agent ne voit pas cette conversation : décris-lui sa tâche en entier ; il te renvoie un résumé. Plusieurs delegate dans un même tour s'exécutent en parallèle.\n{}",
            list.join("\n")
        ));
    }
    if mode == "plan" {
        s.push_str("\nMode Plan : tu es en lecture seule. Explore, puis présente un plan détaillé sans rien modifier.");
    }
    if let Some(ws) = workspace {
        s.push_str(&project::context(ws));
    }
    s.push_str(&skills::prompt_section(skills, query, skills::index_budget(ctx)));
    if let Some(ws) = workspace {
        if let Ok(text) = std::fs::read_to_string(ws.join("AGENTS.md")) {
            let mut text = text;
            text.truncate(text.char_indices().nth(8000).map_or(text.len(), |(i, _)| i));
            s.push_str("\n\nInstructions du projet (AGENTS.md) :\n");
            s.push_str(&text);
        }
    }
    s
}

/// Taille (en caractères) de ce que l'agent ajoute au contexte avant la conversation.
#[derive(serde::Serialize, Default)]
pub struct ContextReport {
    pub system: usize,
    pub skills: usize,
    pub tools: usize,
    pub mcp: usize,
}

/// Mesure le prompt système, les skills et les définitions d'outils tels que `run` les enverrait.
pub fn context_report(workspace: Option<PathBuf>, mode: &str, web: bool, skills: Vec<Skill>, ctx: usize, hub: &McpHub) -> ContextReport {
    let plan = mode == "plan";
    let skills_text = skills::prompt_section(&skills, "", skills::index_budget(ctx));
    let params = RunParams {
        request_id: String::new(),
        model: String::new(),
        messages: vec![],
        workspace: workspace.clone(),
        mode: mode.into(),
        options: None,
        skills,
        web,
        profile: None,
        agents: agents::builtins(),
        depth: 0,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    let system = system_prompt(&params, "", ctx).len().saturating_sub(skills_text.len());
    let mut tool_defs = file_tool_definitions(&params);
    tool_defs.extend(meta_definitions(&params));
    let size = |v: &[Value]| v.iter().map(|d| d.to_string().len()).sum::<usize>();
    ContextReport {
        system,
        skills: skills_text.len(),
        tools: size(&tool_defs),
        mcp: if plan { 0 } else { size(&hub.definitions()) },
    }
}
