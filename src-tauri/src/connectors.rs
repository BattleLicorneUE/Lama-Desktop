//! Catalogue de connecteurs MCP prêts à l'emploi : on remplit quelques champs, l'entrée est ajoutée à `mcp.json`.
//! Les jetons et mots de passe vont dans le coffre Windows ; `mcp.json` ne contient qu'un renvoi `${secret:…}`.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize, Clone)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub help: &'static str,
    /// Jeton, clé ou mot de passe : gardé dans le coffre Windows.
    pub secret: bool,
    /// Champ qui désigne un dossier ou un fichier de ce PC.
    pub path: bool,
    pub optional: bool,
}

#[derive(Serialize, Clone)]
pub struct Connector {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub category: &'static str,
    /// Prérequis à installer (Node.js, uv).
    pub needs: &'static str,
    pub command: &'static str,
    /// Arguments ; `{champ}` est remplacé par la valeur saisie.
    pub args: &'static [&'static str],
    /// Variables d'environnement ; `{champ}` est remplacé par la valeur saisie.
    pub env: &'static [(&'static str, &'static str)],
    pub fields: Vec<Field>,
    /// Étapes à faire une fois pour obtenir le jeton ou l'accès.
    pub setup: &'static str,
    pub docs: &'static str,
    /// Serveur déjà lancé joint par HTTP (`{champ}` remplacé comme pour les arguments) ; vide pour un serveur lancé par l'app.
    pub url: &'static str,
}

fn f(key: &'static str, label: &'static str, placeholder: &'static str, help: &'static str, secret: bool, path: bool, optional: bool) -> Field {
    Field { key, label, placeholder, help, secret, path, optional }
}

fn c(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    category: &'static str,
    needs: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    env: &'static [(&'static str, &'static str)],
    fields: &[Field],
    setup: &'static str,
    docs: &'static str,
) -> Connector {
    Connector { id, name, desc, category, needs, command, args, env, fields: fields.to_vec(), setup, docs, url: "" }
}

/// Connecteur vers un serveur MCP HTTP déjà lancé (par exemple celui intégré à un éditeur).
fn web(id: &'static str, name: &'static str, desc: &'static str, category: &'static str, url: &'static str, fields: &[Field], setup: &'static str, docs: &'static str) -> Connector {
    Connector { id, name, desc, category, needs: "", command: "", args: &[], env: &[], fields: fields.to_vec(), setup, docs, url }
}

pub fn catalog() -> Vec<Connector> {
    vec![
        c("fetch", "Pages web", "Lire n'importe quelle page web et la convertir en texte.", "Essentiels", "uv", "uvx", &["mcp-server-fetch"], &[], &[], "", "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch"),
        c("time", "Heure et fuseaux", "Heure exacte dans n'importe quelle ville, conversions de fuseaux.", "Essentiels", "uv", "uvx", &["mcp-server-time"], &[], &[], "", "https://github.com/modelcontextprotocol/servers/tree/main/src/time"),
        c("memory", "Mémoire (graphe)", "Graphe de connaissances persistant entre les conversations.", "Essentiels", "Node.js", "npx", &["-y", "@modelcontextprotocol/server-memory"], &[], &[], "", "https://github.com/modelcontextprotocol/servers/tree/main/src/memory"),
        c("sequential-thinking", "Raisonnement par étapes", "Aide le modèle à décomposer un problème complexe.", "Essentiels", "Node.js", "npx", &["-y", "@modelcontextprotocol/server-sequential-thinking"], &[], &[], "", "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking"),
        c(
            "brave-search", "Recherche Brave", "Recherche web et locale via l'API de Brave.", "Web", "Node.js", "npx",
            &["-y", "@modelcontextprotocol/server-brave-search"], &[("BRAVE_API_KEY", "{key}")],
            &[f("key", "Clé API Brave", "BSA…", "Clé gratuite (2 000 requêtes par mois).", true, false, false)],
            "1. Crée un compte sur brave.com/search/api.\n2. Choisis l'offre gratuite « Data for Search ».\n3. Copie la clé API affichée.", "https://brave.com/search/api/",
        ),
        c(
            "playwright", "Navigateur automatisé", "Ouvrir des sites, cliquer, remplir des formulaires, lire des pages dynamiques.", "Web", "Node.js", "npx",
            &["-y", "@playwright/mcp@latest"], &[], &[], "Le premier lancement télécharge un navigateur (plusieurs centaines de Mo).", "https://github.com/microsoft/playwright-mcp",
        ),
        c(
            "notion", "Notion", "Chercher, lire et modifier des pages et des bases Notion.", "Équipe", "Node.js", "npx",
            &["-y", "@notionhq/notion-mcp-server"], &[("NOTION_TOKEN", "{token}")],
            &[f("token", "Jeton d'intégration", "ntn_… ou secret_…", "Le jeton de ton intégration Notion.", true, false, false)],
            "1. Sur notion.so/profile/integrations, crée une intégration interne.\n2. Copie le « Internal Integration Secret ».\n3. Dans Notion, ouvre chaque page à rendre accessible, menu « … », « Connexions », ajoute ton intégration.", "https://www.notion.so/profile/integrations",
        ),
        c(
            "unity", "Unity", "Piloter l'éditeur Unity : scènes, objets, composants, scripts, assets, builds. Beaucoup d'outils (≈ 48) : ils occupent une bonne part du contexte du modèle.", "Jeu vidéo", "uv", "uvx",
            &["--from", "mcpforunityserver", "mcp-for-unity", "--transport", "stdio"], &[], &[],
            "1. Dans Unity (2021.3 ou plus récent) : Window, Package Manager, « + », « Add package from git URL », colle https://github.com/CoplayDev/unity-mcp.git?path=/MCPForUnity#main\n2. Garde l'éditeur ouvert : le plugin se connecte tout seul au serveur (vérifie l'état dans Window, MCP for Unity).\n3. Ajoute ce connecteur ici.", "https://github.com/CoplayDev/unity-mcp",
        ),
        c(
            "blender", "Blender", "Piloter Blender : lire la scène, lancer du code Python (bpy), faire des rendus et consulter la documentation officielle. Prévois un bon modèle : il écrit du code Python pour agir.", "Création 3D", "uv", "uvx",
            &["--from", "git+https://projects.blender.org/lab/blender_mcp.git#subdirectory=mcp", "blender-mcp"], &[], &[],
            "1. Il faut Git et uv installés sur ce PC (le serveur est téléchargé au premier lancement, ça peut prendre une minute).\n2. Dans Blender : Édition, Préférences, Extensions, « Dépôts », ajoute le dépôt https://lab.blender.org/ (menu « + »).\n3. Cherche l'extension « MCP » dans la liste, installe-la et active-la.\n4. Garde Blender ouvert avec ta scène, puis ajoute ce connecteur.", "https://www.blender.org/lab/mcp-server/",
        ),
        web(
            "unreal-58", "Unreal Engine 5.8 (officiel)", "Serveur MCP intégré à l'éditeur : acteurs, niveaux, matériaux, Blueprints, tests. Le modèle découvre les outils à la demande (3 outils seulement dans son contexte).", "Jeu vidéo",
            "http://127.0.0.1:{port}/mcp",
            &[f("port", "Port du serveur", "8000", "Celui des préférences de l'éditeur (8000 par défaut).", false, false, true)],
            "1. Dans Unreal 5.8 : Edit, Plugins, active « Unreal MCP » et « All Toolsets », puis redémarre l'éditeur.\n2. Démarre le serveur : Edit, Editor Preferences, General, Model Context Protocol, coche « Auto Start Server » (ou tape ModelContextProtocol.StartServer dans la console de l'éditeur).\n3. Garde le projet ouvert, puis ajoute ce connecteur. Le serveur n'écoute que sur ce PC et n'a pas de mot de passe ; il exécute les outils l'un après l'autre.", "https://dev.epicgames.com/documentation/unreal-engine/unreal-mcp-in-unreal-editor",
        ),
        c(
            "godot", "Godot", "Lancer l'éditeur et les projets Godot, lire les erreurs, créer des scènes et des nœuds.", "Jeu vidéo", "Node.js", "npx",
            &["-y", "@coding-solo/godot-mcp"], &[("GODOT_PATH", "{godot}")],
            &[f("godot", "Exécutable Godot", "C:\\Godot\\Godot_v4.4-stable_win64.exe", "Facultatif : l'application le cherche toute seule, mais indique-le s'il n'est pas dans un dossier habituel.", false, true, true)],
            "Rien à installer dans Godot : le serveur lance l'éditeur lui-même. Indique le chemin de Godot.exe seulement s'il n'est pas trouvé.", "https://github.com/Coding-Solo/godot-mcp",
        ),
    ]
}

// ------------------------------------------------------------------ serveurs MCP personnalisés

/// Découpe une ligne de commande en mots ; les guillemets doubles regroupent un chemin avec des espaces.
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Identifiant d'un serveur : minuscules, chiffres, tirets (les espaces deviennent des tirets).
pub fn clean_id(name: &str) -> Result<String, String> {
    let id: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() && c.is_ascii() || c == '_' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() || id.len() > 40 {
        return Err("Donne un nom de 1 à 40 caractères (lettres, chiffres, tirets).".into());
    }
    Ok(id)
}

/// Une valeur à ranger dans le coffre plutôt que dans `mcp.json` (jeton, clé, mot de passe…).
fn looks_secret(key: &str) -> bool {
    let k = key.to_lowercase();
    ["key", "token", "secret", "password", "passwd", "auth", "bearer", "cookie"].iter().any(|w| k.contains(w))
}

/// Lignes `NOM=valeur` (variables d'environnement) ou `Nom: valeur` (en-têtes HTTP).
pub fn parse_pairs(text: &str, http: bool) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let split = if http { line.find(':').or_else(|| line.find('=')) } else { line.find('=') };
        let i = split.ok_or_else(|| format!("Ligne illisible : « {line} » ({}).", if http { "Nom: valeur" } else { "NOM=valeur" }))?;
        let (k, v) = (line[..i].trim(), line[i + 1..].trim());
        if k.is_empty() || k.contains(char::is_whitespace) || v.contains('\n') {
            return Err(format!("Nom invalide : « {k} »."));
        }
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

/// Entrée de `mcp.json` d'un serveur personnalisé, et les secrets à ranger dans le coffre (nom, valeur).
/// `target` : la ligne de commande (serveur local) ou l'adresse (serveur HTTP).
pub fn custom_entry(id: &str, http: bool, target: &str, pairs: &[(String, String)], all_secret: bool) -> Result<(Value, Vec<(String, String)>), String> {
    let target = target.trim();
    let mut secrets: Vec<(String, String)> = Vec::new();
    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        if v.contains("${secret:") {
            map.insert(k.clone(), json!(v));
        } else if all_secret || looks_secret(k) {
            let name = format!("mcp:{id}:{k}");
            secrets.push((name.clone(), v.clone()));
            map.insert(k.clone(), json!(format!("${{secret:{name}}}")));
        } else {
            map.insert(k.clone(), json!(v));
        }
    }
    let entry = if http {
        if !(target.starts_with("http://") || target.starts_with("https://")) {
            return Err("L'adresse doit commencer par http:// ou https://".into());
        }
        let mut e = json!({ "type": "http", "url": target });
        if !map.is_empty() {
            e["headers"] = Value::Object(map);
        }
        e
    } else {
        let mut words = split_args(target);
        if words.is_empty() {
            return Err("Indique la commande à lancer (par exemple : npx -y @scope/serveur).".into());
        }
        let command = words.remove(0);
        let mut e = json!({ "command": command, "args": words });
        if !map.is_empty() {
            e["env"] = Value::Object(map);
        }
        e
    };
    Ok((entry, secrets))
}

/// Serveurs d'une configuration collée (format Claude Desktop, Claude Code ou Cursor). Les valeurs qui ressemblent à des secrets
/// (clés, jetons, mots de passe) sont rangées dans le coffre.
pub fn import_servers(text: &str) -> Result<Vec<(String, Value, Vec<(String, String)>)>, String> {
    let root: Value = serde_json::from_str(text.trim()).map_err(|e| format!("JSON invalide : {e}"))?;
    let servers = root.get("mcpServers").or_else(|| root.get("servers")).unwrap_or(&root);
    let map = servers.as_object().ok_or("Format inconnu : attendu { \"mcpServers\": { \"nom\": { … } } }.")?;
    let mut out = Vec::new();
    for (name, cfg) in map {
        if !cfg.is_object() {
            continue;
        }
        let id = clean_id(name)?;
        let get_pairs = |key: &str| -> Vec<(String, String)> {
            cfg.get(key).and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect()).unwrap_or_default()
        };
        let (entry, secrets) = if let Some(url) = cfg.get("url").and_then(Value::as_str) {
            custom_entry(&id, true, url, &get_pairs("headers"), false)?
        } else if let Some(command) = cfg.get("command").and_then(Value::as_str) {
            let args: Vec<String> = cfg.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            let (mut entry, secrets) = custom_entry(&id, false, "x", &get_pairs("env"), false)?;
            entry["command"] = json!(command);
            entry["args"] = json!(args);
            (entry, secrets)
        } else {
            return Err(format!("« {name} » : ni « command » ni « url »."));
        };
        out.push((id, entry, secrets));
    }
    if out.is_empty() {
        return Err("Aucun serveur trouvé dans ce texte.".into());
    }
    Ok(out)
}

/// Noms des secrets du coffre cités par une entrée (`${secret:…}`).
pub fn secret_names(entry: &Value) -> Vec<String> {
    let text = entry.to_string();
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("${secret:") {
        let after = &rest[i + "${secret:".len()..];
        let Some(end) = after.find('}') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

/// Noms des champs `{nom}` cités dans un modèle.
fn template_fields(tpl: &str) -> Vec<String> {
    tpl.split('{').skip(1).filter_map(|r| r.split('}').next()).map(String::from).collect()
}

/// Entrée de `mcp.json` pour un connecteur, et les secrets à ranger dans le coffre (nom, valeur).
pub fn build_entry(conn: &Connector, values: &HashMap<String, String>) -> Result<(Value, Vec<(String, String)>), String> {
    let mut secrets: Vec<(String, String)> = Vec::new();
    let mut resolved: HashMap<&str, String> = HashMap::new();
    for fld in &conn.fields {
        let v = values.get(fld.key).map(|s| s.trim()).unwrap_or("");
        if v.is_empty() {
            if fld.optional {
                continue;
            }
            return Err(format!("Renseigne « {} ».", fld.label));
        }
        if v.contains('\0') || v.contains('\n') {
            return Err(format!("« {} » contient un caractère invalide.", fld.label));
        }
        if fld.secret {
            let name = format!("mcp:{}:{}", conn.id, fld.key);
            secrets.push((name.clone(), v.to_string()));
            resolved.insert(fld.key, format!("${{secret:{name}}}"));
        } else {
            resolved.insert(fld.key, v.to_string());
        }
    }
    // Connecteur HTTP : les champs facultatifs laissés vides prennent leur valeur d'exemple (ex. le port par défaut).
    if !conn.url.is_empty() {
        for fld in conn.fields.iter().filter(|f| f.optional) {
            resolved.entry(fld.key).or_insert_with(|| fld.placeholder.to_string());
        }
    }
    let fill = |s: &str| -> String {
        let mut out = s.to_string();
        for (k, v) in &resolved {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        out
    };
    if !conn.url.is_empty() {
        return Ok((json!({ "type": "http", "url": fill(conn.url) }), secrets));
    }
    let args: Vec<String> = conn.args.iter().map(|a| fill(a)).collect();
    // Une variable d'environnement n'est écrite que si tous les champs qu'elle utilise ont été renseignés.
    let env: serde_json::Map<String, Value> = conn
        .env
        .iter()
        .filter(|(_, tpl)| template_fields(tpl).iter().all(|k| resolved.contains_key(k.as_str())))
        .map(|(k, tpl)| (k.to_string(), json!(fill(tpl))))
        .collect();
    let mut entry = json!({ "command": conn.command, "args": args });
    if !env.is_empty() {
        entry["env"] = Value::Object(env);
    }
    Ok((entry, secrets))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(id: &str) -> Connector {
        catalog().into_iter().find(|c| c.id == id).unwrap()
    }

    #[test]
    fn catalog_is_consistent() {
        let all = catalog();
        let mut ids: Vec<&str> = all.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "identifiants uniques");
        for conn in &all {
            if conn.url.is_empty() {
                assert!(["npx", "uvx"].contains(&conn.command), "{}", conn.id);
            } else {
                assert!(conn.url.starts_with("http://127.0.0.1:") || conn.url.starts_with("https://"), "{}", conn.id);
            }
            // Chaque {champ} utilisé existe.
            let used: Vec<&str> = conn.args.iter().chain(std::iter::once(&conn.url)).chain(conn.env.iter().map(|(_, v)| v)).flat_map(|s| s.split('{').skip(1).filter_map(|r| r.split('}').next())).collect();
            for u in used {
                assert!(conn.fields.iter().any(|f| f.key == u), "{} : champ {u} inconnu", conn.id);
            }
        }
    }

    #[test]
    fn secrets_go_to_the_vault_and_the_file_only_has_a_reference() {
        let notion = get("notion");
        let v: HashMap<String, String> = [("token", "ntn_123")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let (entry, secrets) = build_entry(&notion, &v).unwrap();
        let text = entry.to_string();
        assert!(!text.contains("ntn_123"), "{text}");
        assert!(text.contains("${secret:mcp:notion:token}"));
        assert_eq!(secrets, vec![("mcp:notion:token".to_string(), "ntn_123".to_string())]);
    }

    #[test]
    fn missing_required_field_is_reported_and_plain_connectors_need_nothing() {
        let brave = get("brave-search");
        assert!(build_entry(&brave, &HashMap::new()).unwrap_err().contains("Clé API Brave"));
        let (entry, secrets) = build_entry(&get("time"), &HashMap::new()).unwrap();
        assert!(secrets.is_empty());
        assert_eq!(entry["command"], "uvx");
    }

    #[test]
    fn game_engine_connectors_need_no_secret_and_godot_path_is_optional() {
        for id in ["unity", "unreal-58", "godot"] {
            let (entry, secrets) = build_entry(&get(id), &HashMap::new()).unwrap();
            assert!(secrets.is_empty() && entry.get("env").is_none(), "{id} : {entry}");
        }
        let v: HashMap<String, String> = [("godot".to_string(), "C:\\Godot\\godot.exe".to_string())].into();
        let (entry, _) = build_entry(&get("godot"), &v).unwrap();
        assert_eq!(entry["env"]["GODOT_PATH"], "C:\\Godot\\godot.exe");
        assert_eq!(get("unity").args[0], "--from");
        let blender = get("blender");
        assert_eq!((blender.command, blender.category), ("uvx", "Création 3D"));
        assert!(blender.args[1].ends_with("#subdirectory=mcp"));
    }

    #[test]
    fn unreal_58_connector_points_at_the_editor_server_with_default_or_custom_port() {
        let conn = get("unreal-58");
        let (entry, secrets) = build_entry(&conn, &HashMap::new()).unwrap();
        assert!(secrets.is_empty());
        assert_eq!(entry, json!({ "type": "http", "url": "http://127.0.0.1:8000/mcp" }));
        let v: HashMap<String, String> = [("port".to_string(), "8123".to_string())].into();
        assert_eq!(build_entry(&conn, &v).unwrap().0["url"], "http://127.0.0.1:8123/mcp");
    }

    #[test]
    fn unreal_community_connector_is_gone_and_official_one_stays() {
        let ids: Vec<&str> = catalog().iter().map(|c| c.id).collect();
        assert!(!ids.contains(&"unreal") && ids.contains(&"unreal-58"));
    }

    #[test]
    fn command_lines_are_split_with_quotes() {
        assert_eq!(split_args(r#"npx -y "@scope/pkg" --dir "C:\Mes docs""#), vec!["npx", "-y", "@scope/pkg", "--dir", r"C:\Mes docs"]);
        assert!(split_args("   ").is_empty());
        assert_eq!(clean_id(" Mon Serveur Perso! ").unwrap(), "mon-serveur-perso");
        assert!(clean_id("  ").is_err());
    }

    #[test]
    fn custom_servers_keep_secrets_in_the_vault() {
        let pairs = parse_pairs("API_KEY=abc123\nMODE=fast\n# commentaire", false).unwrap();
        let (entry, secrets) = custom_entry("mon", false, "npx -y @x/y --flag", &pairs, false).unwrap();
        assert_eq!(entry["command"], "npx");
        assert_eq!(entry["args"], json!(["-y", "@x/y", "--flag"]));
        assert_eq!(entry["env"]["MODE"], "fast");
        assert!(!entry.to_string().contains("abc123"), "{entry}");
        assert_eq!(secrets, vec![("mcp:mon:API_KEY".to_string(), "abc123".to_string())]);
        assert_eq!(secret_names(&entry), vec!["mcp:mon:API_KEY"]);

        let h = parse_pairs("Authorization: Bearer tok\nX-Region: eu", true).unwrap();
        let (entry, secrets) = custom_entry("web", true, "https://exemple.org/mcp", &h, false).unwrap();
        assert_eq!(entry["type"], "http");
        assert_eq!(entry["headers"]["X-Region"], "eu");
        assert!(!entry.to_string().contains("Bearer tok"));
        assert_eq!(secrets.len(), 1);

        let (all, s2) = custom_entry("tout", false, "uvx pkg", &[("REGION".into(), "eu".into())], true).unwrap();
        assert_eq!((s2.len(), all["env"]["REGION"].as_str().unwrap().starts_with("${secret:")), (1, true));
        assert!(custom_entry("x", true, "ftp://non", &[], false).is_err());
        assert!(custom_entry("x", false, "  ", &[], false).is_err());
        assert!(parse_pairs("pas de séparateur", false).is_err());
    }

    #[test]
    fn pasted_configurations_are_imported_and_secrets_moved() {
        let text = r#"{ "mcpServers": {
            "Mon Notion": { "command": "npx", "args": ["-y", "@notionhq/notion-mcp-server"], "env": { "NOTION_TOKEN": "ntn_x", "LANG": "fr" } },
            "unreal": { "type": "http", "url": "http://127.0.0.1:8000/mcp" } } }"#;
        let list = import_servers(text).unwrap();
        assert_eq!(list.len(), 2);
        let notion = list.iter().find(|(id, _, _)| id == "mon-notion").unwrap();
        assert_eq!(notion.1["args"][1], "@notionhq/notion-mcp-server");
        assert_eq!(notion.1["env"]["LANG"], "fr");
        assert!(!notion.1.to_string().contains("ntn_x") && notion.2.len() == 1);
        let unreal = list.iter().find(|(id, _, _)| id == "unreal").unwrap();
        assert_eq!(unreal.1, json!({ "type": "http", "url": "http://127.0.0.1:8000/mcp" }));
        assert!(import_servers("pas du json").is_err());
        assert!(import_servers(r#"{"mcpServers":{"a":{"truc":1}}}"#).is_err());
    }
}
