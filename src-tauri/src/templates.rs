//! Modèles de projet : quelques fichiers de départ propres, que l'agent complète ensuite.

use std::path::Path;

use serde::Serialize;

#[derive(Serialize)]
pub struct TemplateInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub desc: &'static str,
    /// Première consigne proposée à l'agent une fois le projet créé.
    pub prompt: &'static str,
}

const AGENTS_WEB: &str = "# Règles du projet\n\n- Page web sans dépendances : HTML, CSS et JavaScript simples dans ce dossier.\n- Pas de framework ni de build ; ouvrir index.html suffit.\n- Interface accessible, adaptée au téléphone, avec états vide / erreur.\n";
const AGENTS_NODE: &str = "# Règles du projet\n\n- Node.js sans dépendance externe sauf nécessité.\n- Lancer `npm test` après chaque modification.\n- Code lisible, fonctions courtes, erreurs gérées.\n";
const AGENTS_PY: &str = "# Règles du projet\n\n- Python 3, bibliothèque standard en priorité.\n- Lancer `python -m pytest -q` après chaque modification.\n- Fonctions typées et courtes ; pas de code mort.\n";

pub fn list() -> Vec<TemplateInfo> {
    vec![
        TemplateInfo { id: "web", label: "Page web", desc: "index.html, style.css, app.js : aucune installation.", prompt: "Construis une page web : " },
        TemplateInfo { id: "node-cli", label: "Outil en ligne de commande (Node)", desc: "package.json, point d'entrée, test intégré.", prompt: "Construis un outil en ligne de commande qui " },
        TemplateInfo { id: "node-api", label: "API web (Node)", desc: "Petit serveur HTTP sans dépendances, avec test.", prompt: "Construis une API web qui " },
        TemplateInfo { id: "python", label: "Script Python", desc: "main.py, test pytest, requirements.", prompt: "Écris un programme Python qui " },
    ]
}

fn files(id: &str, name: &str) -> Option<Vec<(&'static str, String)>> {
    Some(match id {
        "web" => vec![
            ("index.html", format!("<!doctype html>\n<html lang=\"fr\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>{name}</title>\n  <link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n  <main>\n    <h1>{name}</h1>\n  </main>\n  <script src=\"app.js\"></script>\n</body>\n</html>\n")),
            ("style.css", "body { font-family: system-ui, sans-serif; margin: 0; line-height: 1.5; }\nmain { max-width: 720px; margin: 0 auto; padding: 24px 16px; }\n".into()),
            ("app.js", "// Point d'entrée de la page.\n".into()),
            ("AGENTS.md", AGENTS_WEB.into()),
        ],
        "node-cli" => vec![
            ("package.json", format!("{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"bin\": \"index.js\",\n  \"scripts\": {{ \"start\": \"node index.js\", \"test\": \"node --test\" }}\n}}\n", slug(name))),
            ("index.js", "#!/usr/bin/env node\nimport { main } from \"./lib.js\";\n\nmain(process.argv.slice(2));\n".into()),
            ("lib.js", "export function main(args) {\n  console.log(\"Bonjour\", args.join(\" \"));\n}\n".into()),
            ("lib.test.js", "import test from \"node:test\";\nimport assert from \"node:assert\";\nimport { main } from \"./lib.js\";\n\ntest(\"main existe\", () => {\n  assert.equal(typeof main, \"function\");\n});\n".into()),
            ("AGENTS.md", AGENTS_NODE.into()),
        ],
        "node-api" => vec![
            ("package.json", format!("{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{ \"start\": \"node server.js\", \"test\": \"node --test\" }}\n}}\n", slug(name))),
            ("server.js", "import http from \"node:http\";\n\nexport function handler(req, res) {\n  res.setHeader(\"content-type\", \"application/json\");\n  if (req.url === \"/health\") return res.end(JSON.stringify({ ok: true }));\n  res.statusCode = 404;\n  res.end(JSON.stringify({ error: \"introuvable\" }));\n}\n\nif (process.argv[1]?.endsWith(\"server.js\")) {\n  http.createServer(handler).listen(3000, () => console.log(\"http://localhost:3000\"));\n}\n".into()),
            ("server.test.js", "import test from \"node:test\";\nimport assert from \"node:assert\";\nimport { handler } from \"./server.js\";\n\ntest(\"health\", () => {\n  let body = \"\";\n  const res = { setHeader() {}, end(b) { body = b; } };\n  handler({ url: \"/health\" }, res);\n  assert.deepEqual(JSON.parse(body), { ok: true });\n});\n".into()),
            ("AGENTS.md", AGENTS_NODE.into()),
        ],
        "python" => vec![
            ("main.py", "def main() -> None:\n    print(\"Bonjour\")\n\n\nif __name__ == \"__main__\":\n    main()\n".into()),
            ("tests/test_main.py", "from main import main\n\n\ndef test_main_runs(capsys):\n    main()\n    assert \"Bonjour\" in capsys.readouterr().out\n".into()),
            ("requirements.txt", "pytest\n".into()),
            ("AGENTS.md", AGENTS_PY.into()),
        ],
        _ => return None,
    })
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "projet".into() } else { s }
}

/// Crée les fichiers du modèle dans `dir` (créé au besoin). Refuse d'écraser un fichier existant.
pub fn create(id: &str, dir: &Path) -> Result<usize, String> {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Projet".into());
    let list = files(id, &name).ok_or_else(|| format!("Modèle inconnu : {id}"))?;
    if let Some((f, _)) = list.iter().find(|(f, _)| dir.join(f).exists()) {
        return Err(format!("« {f} » existe déjà dans ce dossier : choisis un dossier vide."));
    }
    for (f, content) in &list {
        let p = dir.join(f);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, content).map_err(|e| format!("Écriture de {f} impossible : {e}"))?;
    }
    Ok(list.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_creates_files_once() {
        for t in list() {
            let d = std::env::temp_dir().join(format!("tpl-{}-{}", t.id, uuid::Uuid::new_v4()));
            let n = create(t.id, &d).unwrap();
            assert!(n >= 3 && d.join("AGENTS.md").exists(), "{}", t.id);
            assert!(create(t.id, &d).is_err(), "pas d'écrasement");
            let _ = std::fs::remove_dir_all(d);
        }
        assert!(create("nimporte", &std::env::temp_dir()).is_err());
    }
}
