# Architecture

Lama Desktop est une application **Tauri 2** : un cœur en **Rust** (`src-tauri/`) et une interface **SvelteKit / Svelte 5** (`src/`) qui communiquent par des commandes Tauri (`invoke`).

```
src/                    Interface (SvelteKit, mode SPA)
  lib/*.svelte          Panneaux, messages, réglages, coffre, tutoriel…
  lib/chat.svelte.ts    État des conversations et lancement de l'agent
  lib/artifacts.ts      Aperçus isolés (HTML, SVG, React, Mermaid)
  lib/markdown.ts       Markdown → HTML nettoyé (DOMPurify)
src-tauri/src/          Cœur Rust
  lib.rs                Commandes Tauri et état de l'application
  agent/                Boucle de l'agent, outils, règles d'approbation, bac à sable
  vault.rs              Coffre chiffré (Argon2id + XChaCha20-Poly1305)
  browser.rs            Navigateur piloté par le protocole Chrome DevTools
  winio.rs              Saisie clavier et presse-papiers Windows
  history.rs            Conversations et mémoire (SQLite)
  schedules.rs          Tâches planifiées
  backup.rs             Sauvegarde, restauration, effacement
  web.rs                Recherche et lecture web (adresses internes refusées)
  mcp.rs                Connecteurs MCP
  sync.rs               Verrous qui survivent à un panic
```

## Principes de sécurité

- **Le modèle ne voit jamais un secret.** Le coffre ne donne à l'agent que des titres ; la saisie passe par l'application, après accord de l'utilisateur.
- **Bac à sable de fichiers** (`agent/sandbox.rs`) : tout chemin est résolu puis refusé s'il sort du dossier de travail (`..`, liens symboliques, chemins absolus). `.git` est protégé.
- **Commandes** : en mode Auto, seule une liste blanche s'exécute sans demande ; les commandes qui lancent du code du projet demandent un accord par dossier.
- **Web** (`web.rs`) : l'adresse est résolue puis figée avant la requête ; les adresses locales et privées sont refusées, redirections comprises.
- **Contenu généré** : le Markdown passe par DOMPurify (images externes et `url()` CSS retirés) ; les aperçus tournent dans une iframe `sandbox="allow-scripts"` sans accès à l'application.
- **Sauvegardes** : les secrets (clé API, jetons) restent dans le coffre Windows et ne sont jamais écrits dans une archive ; `mcp.json` n'est restauré qu'avec accord explicite.

## Données

Chaque profil a son dossier dans le répertoire de données de l'application : base SQLite (`history.db`), configuration MCP, compétences, profil du navigateur de Lama. Les secrets sont dans le coffre du système.

## Plateforme

Le cœur Rust utilise des API Windows (`winio.rs`, `webprint.rs`, `std::os::windows`) : l'application se compile et se teste sous **Windows**. L'interface seule peut tourner dans un navigateur avec `npm run dev` (un faux backend est fourni dans `src/lib/dev-mock.ts`).
