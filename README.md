# Lama Desktop

Application de bureau pour Windows qui sert de poste de travail avec les modèles **Ollama Cloud** : discussion, projets, agent capable de travailler sur ton ordinateur, gestionnaire de mots de passe, tâches planifiées et plus encore. Tout se passe dans une seule fenêtre, en français.

> Projet personnel en version 0.1.0. Windows uniquement pour l'instant.

## Ce que fait l'application

### Discuter
- Conversations avec les modèles d'Ollama Cloud, réponses affichées au fur et à mesure.
- Modèle par défaut : `beam`. Un guide classe les modèles par usage et par prix pour aider à choisir.
- Markdown, code coloré, formules mathématiques, images et fichiers joints.
- Recherche sur le web activée par défaut.
- Mode « réflexion » repliable pour les modèles qui l'offrent.

### Projets
- Un projet regroupe un dossier de travail, des instructions, des documents et une mémoire.
- Les documents du projet sont indexés : le modèle retrouve les passages utiles (recherche BM25, insensible aux accents).
- Mémoire entre conversations : Lama retient ce que tu lui demandes de retenir, pour tout le profil ou pour un projet précis. Il peut aussi rechercher dans tes anciennes conversations.

### Agent
Lama peut agir, pas seulement répondre :
- lire, écrire et modifier des fichiers dans ton dossier de travail ;
- lancer des commandes, avec ton accord selon le mode choisi ;
- piloter l'ordinateur (clavier, souris, fenêtres) ;
- utiliser des **compétences** (skills), des **sous-agents** et des serveurs **MCP** (connecteurs) ;
- afficher des **artefacts** (pages, applications) en aperçu.

Trois niveaux de permission : demander à chaque fois, automatique (demande une fois par dossier avant de lancer du code du projet), ou ignorer les permissions. Les actions sensibles, comme remplir un mot de passe, demandent toujours ton accord.

### Gestionnaire de mots de passe et infos rapides
- Coffre chiffré par une phrase secrète maîtresse (Argon2 + ChaCha20-Poly1305). Rien n'est stocké en clair.
- Générateur de mots de passe, indicateur de solidité.
- **Infos rapides** : adresse, téléphone, etc. que Lama peut utiliser pour remplir un formulaire.
- Le modèle ne voit jamais un mot de passe. Il demande « remplis ce champ » ou « connecte-moi à tel site », l'application te montre la demande, puis saisit elle-même la valeur.
- Verrouillage automatique après inactivité. L'accès de l'agent au coffre est désactivé par défaut.
- **Navigateur de Lama** : un Edge, Chrome ou Brave séparé que Lama pilote pour te connecter à un site. Il vérifie que la page est bien celle du site enregistré avant d'envoyer quoi que ce soit.

### Tâches planifiées
Programme une demande à une heure ou à un rythme donné ; Lama la lance seul et te prévient quand c'est terminé.

### Notifications Windows
Une notification apparaît quand Lama a fini de répondre ou de travailler et que la fenêtre n'est pas au premier plan.

### Profils, tutoriel et progression
- Plusieurs profils, chacun avec ses conversations, ses réglages et son coffre.
- Au premier lancement : création du profil, puis un court tutoriel. La clé API n'est demandée qu'à la fin.
- Petite gamification : points d'expérience, niveaux, série de jours et badges.
- Animations d'ouverture et de transition.

### Données
- Sauvegarde et restauration complètes dans un fichier zip.
- Export des conversations en Markdown.
- Page d'utilisation (messages et modèles utilisés).
- Option « Tout effacer » pour supprimer toutes les données de l'application.

## Installation

### Utiliser l'application
Prends l'installeur Windows (`Lama Desktop_0.1.0_x64-setup.exe` ou le `.msi`) généré par la compilation, voir ci-dessous. Il faut un compte [Ollama](https://ollama.com) et une clé API, qui est demandée à la fin du tutoriel.

### Compiler soi-même
Prérequis : [Node.js](https://nodejs.org) LTS, [Rust](https://rustup.rs) avec les outils de compilation C++ de Visual Studio, et WebView2 (déjà présent sur Windows 11).

```bash
npm install
npm run tauri dev      # lancement en développement
npm run tauri build    # installeurs dans src-tauri/target/release/bundle/
```

Vérifications :

```bash
npm run check                          # types Svelte / TypeScript
cargo test --lib --manifest-path src-tauri/Cargo.toml
```

## Mises à jour automatiques

L'application vérifie au lancement (et dans Réglages → Mises à jour) s'il existe une version plus récente dans les [Releases](https://github.com/BattleLicorneUE/Lama-Desktop/releases) du dépôt. Elle la télécharge, vérifie sa signature, s'installe et se relance. Les données des utilisateurs ne sont pas touchées.

Pour publier une version :

1. Change le numéro de version dans `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` et `package.json`.
2. Commite, puis envoie un tag :

```bash
git tag v0.2.0
git push origin main v0.2.0
```

3. Le workflow `.github/workflows/release.yml` compile, signe et publie l'installeur et `latest.json`.

Mise en place unique : ajouter dans Settings → Secrets and variables → Actions les secrets `TAURI_SIGNING_PRIVATE_KEY` (contenu du fichier de clé privée) et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (vide si la clé n'a pas de mot de passe). Le dépôt doit être public pour que les applications puissent lire les Releases. Ne perds jamais la clé privée et ne la mets jamais dans le dépôt.

## Technologies
- **Tauri 2** et **Rust** pour le cœur de l'application (agent, coffre, navigateur, base SQLite).
- **SvelteKit** et **Svelte 5** pour l'interface.
- Le navigateur de Lama est piloté par le protocole Chrome DevTools.

## Organisation du code

| Dossier | Contenu |
|---|---|
| `src/` | Interface Svelte (`src/lib/` : panneaux, messages, réglages, coffre, tutoriel…) |
| `src-tauri/src/agent/` | Boucle de l'agent, outils, règles d'approbation |
| `src-tauri/src/vault.rs` | Coffre chiffré |
| `src-tauri/src/browser.rs` | Navigateur piloté par Lama |
| `src-tauri/src/winio.rs` | Saisie clavier et presse-papiers Windows |
| `src-tauri/src/history.rs` | Conversations et mémoire (SQLite) |
| `src-tauri/src/schedules.rs` | Tâches planifiées |
| `src-tauri/src/backup.rs` | Sauvegarde, restauration, effacement |
| `src-tauri/src/web.rs` | Recherche et lecture web, protégées contre les adresses internes |

## Sécurité
- Les clés et secrets sont stockés dans le coffre du système (Windows), jamais dans le dépôt.
- Les pages affichées sont nettoyées (DOMPurify) et la politique de sécurité du contenu est restrictive.
- Les accès web refusent les adresses locales et privées, y compris après redirection.
- Aucune télémétrie.

Pour signaler un problème de sécurité, ouvre une issue sans y mettre d'information sensible.
