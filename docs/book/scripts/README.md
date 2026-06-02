# Scripts mdBook

## `gen-sitemap.sh`

Génère `sitemap.xml` + `robots.txt` dans le build mdBook. À appeler après
`mdbook build docs/book` parce que mdBook n'a pas de sitemap natif.

### Usage local

```bash
mdbook build docs/book
bash docs/book/scripts/gen-sitemap.sh book.apollia.fr docs/target/book
```

### Usage CI/CD

Déjà câblé dans `.github/workflows/ci.yml` (job `book`), step `Generate
sitemap.xml + robots.txt` après le build mdBook.

### Usage Cloudflare Pages

Dans **Cloudflare Pages → Settings → Builds & deployments → Build
configurations**, configurer :

- **Build command** : `mdbook build docs/book && bash docs/book/scripts/gen-sitemap.sh book.apollia.fr docs/target/book`
- **Build output directory** : `docs/target/book`
- **Root directory** : (vide, racine du repo)

### Vérification post-déploiement

```bash
curl -s -o /dev/null -w "%{http_code}\n" https://book.apollia.fr/sitemap.xml
# Doit retourner 200

curl -s https://book.apollia.fr/sitemap.xml | xmllint --noout -
# Doit retourner sans erreur
```
