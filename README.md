# Lama Desktop

[![Vérifications](https://github.com/BattleLicorneUE/Lama-Desktop/actions/workflows/ci.yml/badge.svg)](https://github.com/BattleLicorneUE/Lama-Desktop/actions/workflows/ci.yml)
[![Dernière version](https://img.shields.io/github/v/release/BattleLicorneUE/Lama-Desktop)](https://github.com/BattleLicorneUE/Lama-Desktop/releases/latest)
[![Licence MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](LICENSE)

Application de bureau pour Windows qui sert de poste de travail avec les modèles **Ollama Cloud** : discussion, projets, agent capable de travailler sur ton ordinateur, gestionnaire de mots de passe, tâches planifiées et plus encore. Tout se passe dans une seule fenêtre, en français.

> Projet personnel (version 0.2.1). Windows uniquement pour l'instant.

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

## Sécurité
- Les clés et secrets sont stockés dans le coffre du système (Windows), jamais dans le dépôt.
- Les pages affichées sont nettoyées (DOMPurify) et la politique de sécurité du contenu est restrictive.
- Les accès web refusent les adresses locales et privées, y compris après redirection.
- Statistiques d'usage : désactivées tant que tu n'as pas dit oui (voir « Statistiques anonymes » ci-dessous).
- Restaurer une sauvegarde ne rétablit les serveurs MCP (qui lancent des programmes) qu'après ton accord explicite ; l'extraction est limitée en taille et ne touche pas au profil si l'archive est invalide.
- Les aperçus (React, Mermaid) utilisent des bibliothèques livrées avec l'application ; les actions de la CI sont épinglées sur un commit précis.

Pour signaler un problème de sécurité, ouvre une issue sans y mettre d'information sensible.

## Statistiques anonymes (facultatif)
Au premier lancement, Lama te demande si tu veux l'aider à s'améliorer. **Sans ton accord, rien n'est envoyé.** Tu peux changer d'avis dans Réglages, Statistiques anonymes.

- **Ce qui part** : le lancement et la fermeture (avec la mémoire et le type de carte graphique du PC, en tranches), le modèle utilisé pour un message ou une tâche d'agent, les fonctions essayées (sauvegarde, tâche planifiée, coffre…), l'installation d'un modèle local et le temps de démarrage du moteur, l'ajout d'un connecteur, les pages de Réglages ouvertes, le type des erreurs de réponse (jamais leur texte) et les plantages internes (fichier et ligne du code).
- **Ce qui ne part jamais** : le texte des conversations, les fichiers, la mémoire, les clés et mots de passe, les noms de projets, le nom du PC ou du compte Windows.
- **Aucun identifiant n'est gardé** d'un lancement à l'autre.
- Le service utilisé est [Aptabase](https://aptabase.com). Les réglages affichent la liste exacte des événements et ce qui a été envoyé depuis le lancement.
- Le code est dans `src-tauri/src/analytics.rs` : tu peux vérifier. Une version compilée sans clé Aptabase (variable `LAMA_APTABASE_KEY`) n'envoie rien du tout.

## Aller plus loin
- [Télécharger la dernière version](https://github.com/BattleLicorneUE/Lama-Desktop/releases/latest)
- [Architecture](docs/ARCHITECTURE.md)
- [Contribuer et publier une version](CONTRIBUTING.md)
- [Journal des changements](CHANGELOG.md)
- [Politique de sécurité](SECURITY.md)

Licence [MIT](LICENSE).
