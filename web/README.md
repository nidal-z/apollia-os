# Apollia — sites de documentation

Trois sites statiques HTML/CSS/JS prêts à déployer sur 3 sous-domaines indépendants.

| Sous-domaine cible | Source | Générateur | Build path |
|---|---|---|---|
| `book.apollia.fr` | `book/src/` | mdBook (Rust) | `target/book/` → `web/dist/book/` |
| `docs.apollia.fr` | `docs/wiki/` | VitePress (Vue/Vite) | `web/wiki-site/.vitepress/dist/` → `web/dist/wiki/` |
| `guide.apollia.fr` | `help/` | VitePress (Vue/Vite) | `web/help-site/.vitepress/dist/` → `web/dist/help/` |

## Build

```bash
# Tout
./scripts/build-docs.sh

# Site par site
./scripts/build-docs.sh book
./scripts/build-docs.sh wiki
./scripts/build-docs.sh help
./scripts/build-docs.sh wiki help     # combinaisons
```

Sortie : `web/dist/{book,wiki,help}/` — chaque dossier est un site statique autonome (à pointer comme `dist` Cloudflare Pages).

## Dev local

```bash
# mdBook (book) — port 3000 par défaut
cd book && mdbook serve --open

# VitePress wiki — port 5174
cd web/wiki-site && npm run dev

# VitePress help — port 5175
cd web/help-site && npm run dev
```

Les 3 sites peuvent tourner simultanément (ports différents).

## URLs cross-site

Les liens entre sites utilisent les domaines configurés via variables d'environnement (avec défauts publics) :

- `BOOK_URL` (défaut `https://book.apollia.fr`)
- `DOCS_URL` (défaut `https://docs.apollia.fr`)
- `HELP_URL` (défaut `https://guide.apollia.fr`)

Pour build avec d'autres URLs (preview ou environnement de staging) :

```bash
DOCS_URL=https://docs-staging.apollia.fr ./scripts/build-docs.sh
```

## Architecture interne

### VitePress (wiki + help)

Chaque site VitePress lit son contenu via un dossier `content/` qui contient des **symlinks** vers les fichiers markdown sources :

- `web/wiki-site/content/*.md` → `docs/wiki/*.md`
- `web/help-site/content/*` → `help/*`

Cette indirection évite des problèmes de résolution SSR quand `srcDir` pointe hors du projet npm. Les symlinks sont préservés via `vite.resolve.preserveSymlinks: true`.

Quand un nouveau fichier est ajouté à `docs/wiki/` ou `help/`, **il faut recréer les symlinks** :

```bash
# Pour le wiki
cd web/wiki-site/content
for f in /Users/.../docs/wiki/*.md; do ln -sfn "$f" "$(basename "$f")"; done

# Pour le help (symlinks de dossiers)
cd web/help-site/content
for d in /Users/.../help/*/; do ln -sfn "$d" "$(basename "$d")"; done
ln -sfn /Users/.../help/index.md index.md
```

Un script utilitaire `scripts/refresh-symlinks.sh` peut être ajouté si nécessaire.

### Particularités markdown

Les configs VitePress activent :

- `html: false` — pour escape les `<Vec<T>>`, `<Option<String>>` et autres generics Rust qui sinon seraient interprétés comme des composants Vue.
- Hook `escape-vue-interpolation` — pour escape les `{{...}}` (templates de pipelines, exemples TOML).
- Hook `<pre v-pre>` — double protection sur les blocs de code.
- `ignoreDeadLinks: true` — temporaire, à durcir une fois les cross-links validés.

### Diagrammes

Les références à `docs/diagrams/*.svg|.puml` ont été converties en URLs GitHub absolues lors du build initial du site. Les diagrammes restent sources de vérité dans `docs/diagrams/` ; pour générer les SVG localement (PlantUML requis) :

```bash
cd docs/diagrams
plantuml *.puml
```

## Hosting Cloudflare Pages

3 projets Cloudflare Pages distincts, chacun pointant vers un sous-domaine et son dossier `dist` :

| Projet | Build command | Output dir | Domaine custom |
|---|---|---|---|
| `apollia-book`   | `./scripts/build-docs.sh book` | `web/dist/book` | `book.apollia.fr` |
| `apollia-docs`   | `./scripts/build-docs.sh wiki` | `web/dist/wiki` | `docs.apollia.fr` |
| `apollia-guide`  | `./scripts/build-docs.sh help` | `web/dist/help` | `guide.apollia.fr` |

Variables d'env à définir dans Cloudflare : `BOOK_URL`, `DOCS_URL`, `HELP_URL` pointant vers les domaines finaux pour générer les bons liens cross-site.

## Limitations connues

1. **Captures d'écran** : les pages help contiennent des placeholders `[SCREENSHOT: ...]`. À remplacer manuellement par de vraies captures (PNG idéalement, à placer dans `help/_assets/screenshots/` puis référencer en markdown).
2. **Liens cross-site internes** : actuellement écrits en URL absolue `https://book.apollia.fr/...`. Si tu déploies sur un autre domaine, override via `BOOK_URL` au build.
3. **Sidebar wiki** : générée manuellement dans `web/wiki-site/.vitepress/sidebar.ts` à partir de `docs/wiki/_Sidebar.md`. Si tu ajoutes une nouvelle page wiki, ajoute-la aussi dans la sidebar TS (ou écris un script de génération auto).
4. **Sidebar help** : générée manuellement dans `web/help-site/.vitepress/sidebar.ts`. Même remarque.

## Recommandations Phase suivante (skill auto-sync)

Le skill Phase C aura besoin d'invoquer `scripts/build-docs.sh` (ou les commandes individuelles) après chaque modification doc pour vérifier que rien n'est cassé. Ajouter une étape "build check" au hook post-commit.

Le linter charte `scripts/lints/charte-doc.sh` doit aussi tourner avant chaque build pour échouer rapidement si une règle L1.4 est violée.
