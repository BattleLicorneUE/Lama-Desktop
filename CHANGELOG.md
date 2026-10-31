# Journal des changements

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versions numérotées selon [SemVer](https://semver.org/lang/fr/).

## [Non publié]

## [0.4.0] - 2026-10-31

### Ajouté
- **Navigateur intégré** : un panneau à droite affiche la vraie page web (WebView2) quand Lama cherche ou lit une page, depuis « Navigateur » dans la barre latérale (Ctrl + L pour l'adresse), ou en cliquant un lien d'une réponse (Ctrl + clic : navigateur par défaut). Navigation privée, adresses publiques seulement, téléchargements et autorisations refusés, aucun accès à l'application. Réglable dans Recherche web.
- Bouton « Voir dans le navigateur » sur les cartes de recherche web.
- Raccourcis : Ctrl + L, F2 (renommer une conversation au clavier) ; la liste des raccourcis des réglages est complète.

### Modifié
- Accessibilité : zone principale et titre de page pour les lecteurs d'écran, navigation nommée, page courante annoncée, contrastes du texte atténué et de la coloration du code relevés (WCAG AA), plus de zone principale en double dans les réglages.
- Le navigateur replie le panneau du mode Code ou la barre latérale si la fenêtre est trop étroite, et les rend à sa fermeture ; les notifications se décalent pour rester visibles.
- Zone de saisie : texte d'aide plus court (plus de retour à la ligne quand le panneau est ouvert).

## [0.3.2] - 2026-10-28

### Ajouté
- **Rédaction avec Lama** pour les tâches planifiées et les projets : une phrase suffit pour remplir le nom et les instructions.
- **Premier lancement simplifié** : un modèle local s'installe en un clic depuis l'écran d'accueil, ou on colle une clé cloud.
- **Tests automatiques** (vitest) de l'interface, lancés par l'intégration continue et avant chaque publication.
- **Éditeur de skills** : « Nouveau skill » ouvre un vrai éditeur (nom, quand l'utiliser, instructions) au lieu d'un fichier à compléter dans le Bloc-notes. On peut partir d'un modèle (relire, résumer, message de commit, expliquer du code) ou décrire son idée en une phrase et laisser Lama rédiger le skill.
- **Modifier et supprimer un skill** depuis la liste ; renommer un skill garde ses fichiers annexes.
- Un skill peut être réservé à la commande `/nom` (Lama ne le choisit pas seul) et rangé pour un seul projet.
- **Éditeur d'agents**, sur le même modèle : modèles de départ, rédaction par Lama à partir d'une idée, choix de ce que l'agent peut faire (tout, lecture seule, lecture et web), modèle imposé facultatif, modification et suppression. Les agents intégrés se copient pour en faire sa propre version.

### Modifié
- **Couleur d'accent assombrie** (contraste de 4,9 avec du blanc au lieu de 3,9) et version plus claire en thème sombre, avec texte foncé : boutons et liens lisibles.
- Les petites étiquettes (10 à 11,5 px) passent à 12 px.
- **Échap** : dans une confirmation, la touche ne ferme plus que la confirmation, pas la fenêtre ouverte derrière.
- Les textes des connecteurs précisent que `uv` est téléchargé par Lama ; seul Node.js reste à installer pour `npx`.
- **Modèles locaux, page refaite** : deux onglets (Installés, À installer), filtres par usage, espace disque utilisé et libre, réglage de la taille de conversation replié par défaut.
- **Plusieurs installations à la suite** : un modèle peut être mis en file pendant qu'un autre se télécharge ; vitesse et temps restant affichés, file modifiable, annulation d'un seul modèle ou de tout.
- **Suppression plus claire** : la confirmation indique l'espace libéré, le résultat s'affiche, et Lama bascule sur un autre modèle si on supprime celui qui était choisi.
- **Téléchargements interrompus** : leur taille est visible et un bouton « Nettoyer » les supprime.

### Corrigé
- **Sécurité** : une politique de sécurité du contenu stricte limite les connexions réseau de l'interface et interdit l'envoi de formulaires vers l'extérieur ; les formulaires HTML contenus dans des réponses ou aperçus sont neutralisés.
- La recherche dans les Réglages tolère les pluriels et les accents.
- Cibles de clic d'au moins 24 px et champs sans nom étiquetés.
- Les descriptions de skills contenant des deux-points ou des guillemets sont écrites sans casser l'en-tête du fichier.
- Les champs de message et de recherche de conversation ont un nom pour les lecteurs d'écran.
- Les modèles dont le nom contient un point (`qwen3-1.7b`, `llama3.2-3b`, `qwen2.5-coder-7b`) ne pouvaient pas être supprimés.
- Supprimer un modèle en cours d'installation est refusé au lieu de laisser un dossier à moitié supprimé ; le réglage de contexte d'un modèle supprimé est oublié.

## [0.3.1] - 2026-10-28

### Ajouté
- **Statistiques anonymes, uniquement si tu acceptes** : une demande au premier lancement et une page dans les Réglages qui liste ce qui peut partir et ce qui a été envoyé. Aucun contenu de conversation, aucun identifiant conservé.
  Elles couvrent le lancement et la fermeture, les modèles utilisés, les fonctions essayées, le démarrage du moteur local, le type des erreurs de réponse (jamais leur texte) et les plantages internes (fichier et ligne seulement).
- **Connecteur Blender** (serveur MCP officiel de Blender Lab) : lire la scène, lancer du code Python, faire des rendus et consulter la documentation. Nécessite l'extension MCP dans Blender et Git.
- **Réglages réorganisés** en six groupes et dix-sept pages, avec une phrase d'explication sur chaque page, une recherche par mots-clés et des textes réécrits.

### Corrigé (sécurité)
- **Dépôts git piégés** : un dépôt récupéré ailleurs pouvait lancer un programme à chaque consultation de son état ou de ses différences (réglages `fsmonitor`, diff externe, filtres de texte). Ces réglages sont désormais neutralisés.
- **Liens vers des fichiers privés** : la revue de code ne suit plus les liens symboliques des fichiers neufs, qui auraient pu envoyer un fichier hors du projet au modèle.
- **Ouverture de fichiers** : un nom comme `programme.exe.` (point ou espace final, ignoré par Windows) passait la liste des fichiers interdits ; la liste est aussi plus complète (`.url`, `.msc`, `.settingcontent-ms`…).
- **Enregistrement de fichiers** : les programmes et les dossiers de démarrage automatique sont refusés.
- **Dossier `.git`** protégé quelle que soit la casse (`.GIT`).
- **Recherche web** : plages réservées (réseau de test, multicast, NAT64) ajoutées aux adresses refusées.

### Corrigé
- **Connecteurs Python sans installation** (Blender, pages web, Unity…) : si `uv` n'est pas sur le PC, Lama télécharge sa propre copie, vérifiée par somme de contrôle, au premier besoin. Si un programme manque (`uvx`, `npx`…), l'erreur le dit clairement au lieu d'échouer en silence.
- Une commande de l'agent arrêtée par le délai arrête maintenant aussi ses sous-processus.
- Les statistiques ouvrent une nouvelle session après une heure d'inactivité, sinon le service refusait les événements d'une application restée ouverte longtemps.

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

[Non publié]: https://github.com/BattleLicorneUE/Lama-Desktop/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/BattleLicorneUE/Lama-Desktop/compare/v0.3.2...v0.4.0
[0.2.1]: https://github.com/BattleLicorneUE/Lama-Desktop/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/BattleLicorneUE/Lama-Desktop/releases/tag/v0.2.0
