# Personnaliser le catalogue MCP

> Pour les power users et administrateurs d'équipe qui veulent ajouter, désactiver ou modifier des entrées du catalogue MCP, sans attendre une release d'Apollia.

## Prérequis

- Vous savez éditer un fichier JSON.
- Vous avez un éditeur de texte.
- Apollia doit être fermé pendant l'édition (la v0.1.0 ne fait pas de hot-reload).

## Le fichier `mcp-overrides.json`

Chemin : `~/.apollia/mcp-overrides.json`.

Format : un objet JSON avec trois clés optionnelles, appliquées dans cet ordre :

1. **`disable`** : retirer des entrées du catalogue.
2. **`override`** : patcher des entrées existantes (deep merge).
3. **`add`** : ajouter de nouvelles entrées.

## Étapes

1. Fermer Apollia Desktop.
2. Créer ou éditer `~/.apollia/mcp-overrides.json` selon les cas d'usage ci-dessous.
3. Sauvegarder.
4. Relancer Apollia Desktop.
5. Ouvrir **Connexions, + Découvrir** et vérifier le résultat.

## Cas d'usage

### Masquer une entrée

```json
{ "disable": ["@modelcontextprotocol/server-puppeteer"] }
```

L'entrée disparaît du catalogue. Les serveurs déjà installés ne sont pas désinstallés.

### Modifier une entrée existante (deep merge)

```json
{
  "override": {
    "@notionhq/notion-mcp-server": {
      "default_requires_approval": false
    }
  }
}
```

Le patch est appliqué par fusion récursive. Les objets sont fusionnés, les scalaires et les tableaux sont remplacés entièrement.

### Ajouter une entrée maison

```json
{
  "add": [
    {
      "package_identifier": "@local/mon-mcp",
      "operator_label": { "fr": "Mon MCP", "en": "My MCP" },
      "description": { "fr": "Serveur interne de mon équipe." },
      "category": "internal",
      "icon_name": "building",
      "trust_level": "custom",
      "default_requires_approval": true,
      "remote_url": "https://mcp.interne.example",
      "remote_transport": "streamable-http",
      "cost_model": { "kind": "free" }
    }
  ]
}
```

Les champs obligatoires sont `package_identifier`, `operator_label`, `category`, `icon_name`, `trust_level`. Le schéma complet (tous les champs disponibles) est documenté dans la référence technique.

## Vérification

- Les entrées de `disable` ne sont plus visibles dans **+ Découvrir**.
- Les entrées de `add` apparaissent avec leur logo et un badge `Custom`.
- Les overrides sont reflétés (par exemple `default_requires_approval=false` rend les outils auto-approuvés).
- Si vous voulez confirmer côté logs, regardez `~/.apollia/logs/runtime.log`, ligne `mcp.catalog.overrides.applied`.

## Si ça ne marche pas

- **Le fichier semble ignoré** : il est probablement mal formé (JSON invalide). Apollia logge un warning `mcp.catalog.overrides.parse_failed` mais ne crashe pas. Validez avec `jq . ~/.apollia/mcp-overrides.json`.
- **Une entrée `add` n'apparaît pas** : un champ obligatoire manque. Les autres entrées du fichier sont quand même appliquées.
- **Vous voulez recharger sans redémarrer** : non supporté en v0.1.0, redémarrage requis.

## Limitations v0.1.0

- Pas de hot-reload.
- Pas de validation cryptographique sur les entrées `add` (vous êtes responsable du contenu).
- Pas de gouvernance multi-utilisateur (PR review), prévu en v0.3 avec un registry remote optionnel.

> **Référence technique :** [Briques-MCP](https://github.com/nidal-z/apollia-os/wiki/Briques-MCP) , schéma complet `ConnectorEnrichment`, ordre d'application, cas particuliers.
