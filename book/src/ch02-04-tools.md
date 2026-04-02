# Utiliser les outils

Dans `run()`, trois services ont été utilisés : `ctx.tools.call("file_read", ...)`, `ctx.tools.call("file_write", ...)`, et `ctx.llm.chat(...)`. Cette section explique comment ils fonctionnent et quelles sont les options disponibles.

---

## ctx.tools — l'interface aux outils

`ctx.tools` est le proxy vers tous les outils déclarés dans votre manifest. Il expose une méthode principale :

```python
result = await ctx.tools.call("nom_outil", {"param": "valeur"})
```

Chaque appel est automatiquement :
- **Vérifié** — seuls les outils déclarés dans `tools_required` ou `tools_optional` sont accessibles
- **Tracé** — enregistré dans l'audit trail SQLite (`~/.apollia/audit.db`)
- **Comptabilisé** — décompté du `step_budget`

Le résultat est toujours un `dict` Python. En cas d'erreur d'exécution (fichier introuvable, timeout, path traversal), le dict contient un champ `"error"` avec un `"code"` machine et un `"message"` lisible.

---

## file_read — lire un fichier

```python
result = await ctx.tools.call("file_read", {
    "path": "/data/rapport.txt",   # chemin absolu ou relatif au sandbox
    "offset": 1,                   # optionnel — ligne de départ (1-based)
    "limit": 100,                  # optionnel — max lignes à retourner
})
```

**Résultat en succès :**
```python
{
    "content": "   1\tLigne 1 du fichier\n   2\tLigne 2...",
    "total_lines": 342,
    "truncated": False
}
```

Le contenu est retourné avec des numéros de ligne préfixés (format `cat -n`). `truncated` est `True` si `limit` a été atteint avant la fin du fichier.

**Résultat en erreur :**
```python
{"error": "NOT_FOUND: /data/rapport.txt introuvable"}
```

**Cas d'usage de la lecture partielle :**

Pour les fichiers volumineux (logs, exports CSV de plusieurs milliers de lignes), lisez par tranches :

```python
# Lire les 100 premières lignes
result = await ctx.tools.call("file_read", {"path": "/data/big.log", "limit": 100})

# Lire les lignes 500 à 600
result = await ctx.tools.call("file_read", {
    "path": "/data/big.log",
    "offset": 500,
    "limit": 100,
})
```

**La protection sandbox :**

`file_read` valide chaque chemin contre la `SandboxRoot` de l'agent avant toute lecture. Un chemin qui tente de sortir du sandbox (`../../etc/passwd`) est rejeté avec le code `TRAVERSAL_ATTEMPTED` — aucun accès disque n'a lieu.

Pour `file-assistant`, les chemins absolus comme `/data/rapport.txt` sont autorisés tant que le fichier est accessible par le processus runtime. La protection est contre les traversals, pas contre les chemins absolus légitimes.

---

## file_write — écrire un fichier

```python
result = await ctx.tools.call("file_write", {
    "path": "/data/rapport_summary.txt",   # crée ou remplace
    "content": "Résumé du rapport...",
})
```

**Résultat en succès :**
```python
{
    "bytes_written": 1247,
    "path": "/data/rapport_summary.txt"
}
```

`file_write` crée le fichier s'il n'existe pas, le remplace s'il existe. L'écriture est **atomique** : Apollia OS écrit dans un fichier temporaire, puis renomme — si le processus est interrompu à mi-écriture, le fichier original n'est pas corrompu.

Les répertoires parents sont créés automatiquement si nécessaire.

---

## Les autres outils fichier

`file_read` et `file_write` ne sont que deux des six outils fichiers natifs. Les voici en bref :

| Outil | Usage |
|---|---|
| `file_read` | Lire un fichier (avec lecture partielle) |
| `file_write` | Écrire ou remplacer un fichier |
| `file_edit` | Remplacer une chaîne exacte dans un fichier |
| `file_list` | Lister les entrées d'un répertoire |
| `file_glob` | Chercher des fichiers par pattern (`**/*.txt`) |
| `file_grep` | Rechercher par expression régulière dans des fichiers |

Le chapitre 4 couvre tous ces outils en détail, avec leurs paramètres complets.

---

## ctx.llm — appeler un LLM

`ctx.llm` est le proxy vers le backend LLM configuré dans `apollia.toml`. Il est `None` si aucun backend n'est configuré.

### Chat simple

Pour la grande majorité des cas : un system prompt + un message utilisateur.

```python
response = await ctx.llm.chat(
    system="Tu es un assistant expert en synthèse de documents...",
    user="Résume ce texte : ...",
)

print(response.content)         # le texte généré
print(response.usage.cost_usd)  # coût en dollars (None pour les backends locaux)
print(response.latency_ms)      # latence en millisecondes
```

### Conversation multi-tour

Pour les échanges avec historique :

```python
response = await ctx.llm.complete([
    {"role": "system",    "content": "..."},
    {"role": "user",      "content": "Première question"},
    {"role": "assistant", "content": "Première réponse"},
    {"role": "user",      "content": "Question de suivi"},
])
```

### Streaming

Pour les réponses longues, récupérez les tokens au fur et à mesure :

```python
chunks = await ctx.llm.stream([
    {"role": "user", "content": "Génère un rapport détaillé..."},
])
full_text = "".join(chunks)
```

`stream()` retourne toujours une `list[str]`. Si le backend ne supporte pas le streaming, un seul chunk est retourné — le code de l'agent ne change pas.

### La boucle ReAct automatique — run_tools()

Pour les agents qui laissent le LLM décider des outils à utiliser, `run_tools()` gère la boucle Thought → Action → Observe automatiquement :

```python
result = await ctx.llm.run_tools(
    messages=[
        {"role": "system", "content": "Tu peux lire des fichiers."},
        {"role": "user",   "content": "Lis rapport.txt et résume-le."},
    ],
    tools=[{
        "name": "file_read",
        "description": "Lit un fichier local.",
        "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
        },
    }],
    max_iterations=5,
)
print(result.content)  # réponse finale après toutes les boucles
```

Le chapitre 6 explique `run_tools()` en profondeur — c'est la base de la boucle ReAct qui fait d'Apollia OS un runtime d'agents autonomes.

---

## Gérer les erreurs d'outils

Un outil peut échouer pour plusieurs raisons : fichier introuvable, timeout, chemin invalide, domaine réseau non autorisé. La convention est uniforme :

```python
result = await ctx.tools.call("file_read", {"path": chemin})

if result.get("error"):
    # result["error"] : str — description lisible de l'erreur
    # Codes machines dans result["error"] : "NOT_FOUND", "TIMEOUT",
    # "TRAVERSAL_ATTEMPTED", "DOMAIN_NOT_ALLOWED"...
    return {
        "task_id": task["task_id"],
        "status": "failed",
        "error": {"code": "FILE_NOT_FOUND", "message": result["error"]},
    }
```

Cette vérification systématique `if result.get("error")` après chaque appel d'outil est une bonne pratique. Elle garantit que votre agent retourne un `status: "failed"` explicite plutôt que de lever une exception Python que le runtime devra attraper.

---

## Récapitulatif des services ctx

| Service | Disponibilité | Rôle |
|---|---|---|
| `ctx.tools` | Toujours disponible | Appeler les outils déclarés dans le manifest |
| `ctx.llm` | `None` si aucun backend configuré | Appels LLM (chat, complete, stream, run_tools) |
| `ctx.memory` | `None` si pas de `memory_namespace` | Mémoire persistante SQLite (chapitre 5) |
| `ctx.step_budget` | Toujours disponible (lecture seule) | Consulter le budget restant |
| `ctx.log` | Toujours disponible | Logs structurés vers le runtime |

La section suivante assemble tout en un fichier complet et montre comment l'exécuter.
