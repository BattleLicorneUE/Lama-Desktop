# Contribuer à Lama Desktop

## Mise en place

Prérequis : [Node.js](https://nodejs.org) LTS, [Rust](https://rustup.rs) avec les outils de compilation C++ de Visual Studio, WebView2 (présent sur Windows 11).

```bash
npm install
npm run tauri dev      # développement
npm run tauri build    # installeurs dans src-tauri/target/release/bundle/
```

## Avant d'ouvrir une pull request

```bash
npm run check                                        # types Svelte / TypeScript
npm run build                                        # build de l'interface
cargo test --lib --manifest-path src-tauri/Cargo.toml
```

La CI (`.github/workflows/ci.yml`) lance les mêmes vérifications sur Windows. Une PR doit être verte pour être fusionnée.

## Branches et pull requests

- Une branche par sujet, nommée `type/sujet-court` (`fix/restauration-zip`, `feat/export-pdf`).
- Une PR petite et ciblée, décrite avec le modèle fourni. Montre comment tu as vérifié ton changement.
- Ne réécris pas l'historique de `main`.

## Messages de commit

Format [Conventional Commits](https://www.conventionalcommits.org/fr/v1.0.0/), en français :

```
type(portée): résumé court à l'impératif

Corps facultatif : le pourquoi plus que le comment.
```

Types : `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `security`.
Exemples : `fix(backup): refuse les archives trop volumineuses`, `docs: ajoute l'architecture`.

## Style

- `.editorconfig` fixe l'indentation et les fins de ligne.
- Les commentaires du code et les messages affichés sont en français ; les noms d'identifiants en anglais.
- Les changements qui touchent au coffre, au bac à sable, à la restauration ou au web doivent s'accompagner de tests (voir `src-tauri/src/*` pour des exemples).

## Publier une version

1. Mets à jour la version dans `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (et `Cargo.lock`), `package.json` et `package-lock.json`, puis complète le [CHANGELOG](CHANGELOG.md).
2. Fusionne dans `main`.
3. Pousse le tag, ou lance le workflow à la main (Actions → *Publier une version* → *Run workflow*, avec le nom du tag) :

```bash
git tag v0.2.2
git push origin v0.2.2
```

Le workflow `.github/workflows/release.yml` vérifie, compile, signe et publie l'installeur et `latest.json` ; les applications installées se mettent à jour toutes seules.

Mise en place unique : dans *Settings → Environments*, crée l'environnement `release` (idéalement avec une validation manuelle) et ajoute les secrets `TAURI_SIGNING_PRIVATE_KEY` et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (vide si la clé n'a pas de mot de passe). Le dépôt doit être public pour que les applications lisent les Releases. Ne perds jamais la clé privée et ne la mets jamais dans le dépôt.
