# Politique de sécurité

## Signaler une vulnérabilité

Ne publie pas de faille dans une issue publique. Utilise le signalement privé de GitHub :
**Security → Report a vulnerability** sur la page du dépôt. Décris le problème, comment le reproduire et son impact.

Tu recevras une réponse dès que possible. Une fois la faille corrigée, elle est mentionnée dans le [CHANGELOG](CHANGELOG.md).

## Versions prises en charge

Seule la dernière version publiée reçoit des correctifs. L'application se met à jour toute seule depuis les [Releases](https://github.com/BattleLicorneUE/Lama-Desktop/releases).

## Ce qui est protégé

Le coffre de mots de passe, le bac à sable de fichiers de l'agent, la restauration de sauvegardes, la lecture du web (adresses internes refusées) et les aperçus d'artefacts (iframe isolée) sont les zones les plus sensibles : tout changement qui les touche doit être testé et relu avec soin. Voir [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
