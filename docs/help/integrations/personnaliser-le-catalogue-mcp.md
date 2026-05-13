# Personnaliser le catalogue MCP

> Pour les operators et les administrateurs d'équipe qui veulent ajouter, désactiver ou modifier des entrées du catalogue MCP sans attendre une release d'Apollia.

## Prérequis

- Vous savez écrire du JSON.
- Vous avez un éditeur de texte.
- Vous savez où trouver votre dossier `~/.apollia/` (créé au premier lancement d'Apollia).

## Pourquoi ce fichier existe

Le catalogue MCP par défaut d'Apollia v0.1.0 contient 18 entrées curées. Pour patcher ce catalogue (ajouter votre serveur interne, désactiver une entrée, modifier un comportement par défaut) sans attendre une release Apollia, déposez un fichier `~/.apollia/mcp-overrides.json`.

## Structure du fichier

Le fichier est un objet JSON avec trois clés optionnelles :

```json
{
  "add": [ ... ],
  "disable": [ ... ],
  "override": { ... }
}
```

### `add` — Ajouter des entrées

Une liste d'entrées au format complet (mêmes champs que les entrées catalogue embarquées). Typiquement pour exposer un serveur MCP interne enterprise :

```json
{
  "add": [
    {
      "package_identifier": "internal-acme-mcp",
      "operator_label": { "en": "ACME Internal", "fr": "ACME Interne" },
      "description": {
        "en": "Internal ACME systems (CRM, billing, support).",
        "fr": "Systèmes internes ACME (CRM, facturation, support)."
      },
      "category": "internal",
      "icon_name": "building",
      "trust_level": "custom",
      "auth_help_url": "https://wiki.acme.internal/mcp",
      "auth_help_text": {
        "en": "Use your ACME SSO token.",
        "fr": "Utilisez votre token SSO ACME."
      },
      "default_requires_approval": true,
      "remote_url": "https://mcp.acme.internal",
      "remote_transport": "streamable-http",
      "cost_model": { "kind": "free" }
    }
  ]
}
```

### `disable` — Masquer des entrées

Une liste de `package_identifier` à retirer du catalogue. Utile si vous ne voulez pas exposer certains serveurs à votre équipe :

```json
{
  "disable": [
    "@anthropic/mcp-server-puppeteer",
    "io.github.github/github-mcp-server"
  ]
}
```

### `override` — Patcher des entrées existantes

Un objet avec `package_identifier` → patch JSON appliqué en deep-merge. Utilisé pour modifier un comportement par défaut sans recréer l'entrée complète :

```json
{
  "override": {
    "io.github.github/github-mcp-server": {
      "default_requires_approval": false
    },
    "@anthropic/mcp-server-filesystem": {
      "description": {
        "fr": "Description personnalisée pour mon équipe."
      }
    }
  }
}
```

Sémantique du merge :

- Les **objets** sont fusionnés récursivement (les clés du patch écrasent les clés homonymes de l'entrée).
- Les **scalaires** (string, number, bool) sont remplacés.
- Les **tableaux** sont remplacés entièrement (pas de concaténation).

## Ordre d'application

1. **disable** s'applique en premier : les entrées listées disparaissent du catalogue.
2. **override** est appliqué ensuite sur les entrées restantes.
3. **add** est appliqué en dernier : les nouvelles entrées sont ajoutées en fin de liste.

## Étapes

1. Créez ou éditez `~/.apollia/mcp-overrides.json` avec le contenu souhaité.
2. Sauvegardez le fichier.
3. Redémarrez Apollia Desktop pour que le fichier soit rechargé (v0.1.0 ne fait pas de hot-reload).
4. Ouvrez **Connexions → catalogue** et vérifiez que vos modifications sont visibles.

## Vérification

- Les entrées listées dans `disable` ne sont plus visibles dans le catalogue.
- Les entrées listées dans `add` apparaissent avec leur logo et leur badge `Custom` (trust level).
- Les overrides sont reflétés (par exemple, `default_requires_approval=false` rend les outils auto-approuvés).
- Dans les logs du runtime : `mcp.catalog.overrides.applied add=N disable=N overrides=N`.

## Si ça ne marche pas

- **Le fichier est ignoré silencieusement** : il est mal formé (JSON invalide). Apollia logge un warning `mcp.catalog.overrides.parse_failed` mais ne crashe pas. Vérifiez avec `jq . ~/.apollia/mcp-overrides.json` ou un linter JSON.
- **Une entrée `add` n'apparaît pas** : un champ obligatoire (`package_identifier`, `operator_label`, `category`, `icon_name`, `trust_level`) manque. Les autres entrées du fichier sont quand même appliquées.
- **L'override de mes paramètres se perd après mise à jour d'Apollia** : non — le fichier `~/.apollia/mcp-overrides.json` est respecté par toutes les versions ultérieures qui partagent ce schéma. Le schéma reste stable cross-paliers (v0.1.0 → v0.3 registry → v0.4 marketplace).

## Cas particulier : serveurs self-hosted

Pour distinguer vos serveurs internes des serveurs officiels SaaS, utilisez `trust_level: "custom"` (ou ajoutez un nouveau niveau dans une release future). Cela apparaît comme un badge spécifique dans l'UI.

## Limitations v0.1.0

- Pas de hot-reload : un changement du fichier nécessite un redémarrage d'Apollia Desktop.
- Pas de validation de signature sur les entrées `add` : vous êtes responsable du contenu que vous ajoutez (cohérent avec une approche power-user).
- La v0.3 introduira un registry remote optionnel (`apollia-mcp-registry`) qui couvre les cas multi-utilisateurs avec gouvernance par PR.

> **Référence technique :** [Briques-MCP](https://github.com/nidal-z/apollia-os/wiki/Briques-MCP)
