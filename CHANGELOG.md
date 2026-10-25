# Journal des changements

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versions numérotées selon [SemVer](https://semver.org/lang/fr/).

## [Non publié]

## [0.3.0] - 2026-10-26

### Ajouté
- **Modèles locaux** (sans Ollama) : installation et suppression de modèles du catalogue depuis Réglages → Système, avec le moteur llama.cpp (Vulkan ou processeur) téléchargé par l'application. Téléchargements repris et vérifiés par SHA-256, un modèle chargé à la fois et déchargé après 15 minutes sans utilisation.
- **Détection du PC** (mémoire, carte graphique, processeur, disque) et recommandation des modèles adaptés (rapide, mixte, processeur, trop lourd).
- **Contexte réglable** pour les modèles locaux : automatique jusqu'à 32 000 jetons selon la carte graphique, jusqu'à 128 000 pour les modèles qui le permettent, cache compressé en 8 bits au-delà de 8 000 jetons.
- Recherche web : Wikipédia en secours quand DuckDuckGo ne répond pas.

### Modifié
- **La recherche web utilise DuckDuckGo pour tous les modèles** et reste active tant que l'ordinateur est en ligne ; l'API web d'Ollama est supprimée. Le bouton marche/arrêt devient un indicateur de connexion.
- La clé API Ollama ne sert plus qu'aux modèles cloud.

### Documentation
- Documentation du dépôt : `CONTRIBUTING.md`, `SECURITY.md`, `docs/ARCHITECTURE.md`, modèles d'issues et de pull requests.
- `.editorconfig` et `.gitattributes` pour des fins de ligne et un style homogènes.

## [0.2.1] - 2026-10-25

### Sécurité
- Restauration de sauvegarde : `mcp.json` (serveurs qui lancent des programmes) n'est restauré qu'après confirmation explicite.
- Restauration atomique : extraction dans un dossier temporaire, profil intact si l'archive est invalide, taille décompressée limitée.
- Aperçus React et Mermaid : bibliothèques livrées avec l'application au lieu d'un CDN.

### Modifié
- Verrous `Mutex` tolérants aux panics (`lock_safe`).
- CI : actions épinglées sur un commit, publication lançable à la main, vérifications à chaque push et pull request, Dependabot mensuel.
- User-Agent des requêtes web suivant la version de l'application.

### Ajouté
- Licence MIT.

## [0.2.0] - 2026-10-25

### Ajouté
- Mises à jour automatiques via les Releases GitHub.
- README en français.

## [0.1.0]

- Première version : discussion, projets, agent, coffre de mots de passe, tâches planifiées, profils.

[Non publié]: https://github.com/BattleLicorneUE/Lama-Desktop/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/BattleLicorneUE/Lama-Desktop/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/BattleLicorneUE/Lama-Desktop/releases/tag/v0.2.0
