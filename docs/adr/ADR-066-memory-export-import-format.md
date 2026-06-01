# ADR-066 - Memory Export/Import : Format JSON Gzip

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 37 (planifié)

---

## Contexte

Apollia OS doit permettre l'export et l'import de la mémoire des agents pour :
- **Migration** : changement de machine, réinstallation
- **Backup** : sauvegarde locale conforme Principe #1
- **Partage** : transfert de mémoire entre instances (ex. staging → production)

La mémoire comprend les trois types : épisodique, sémantique, procédurale - stockés dans `~/.apollia/memory.db` (SQLite + FTS5).

---

## Décision

**Format : JSON ligne par ligne (JSONL) + compression gzip.**

```
~/.apollia/exports/memory_export_2026-04-04T12:00:00Z.jsonl.gz
```

**Schéma JSONL :**
```json
{"version": 1, "exported_at": "2026-04-04T12:00:00Z", "agent_count": 3}
{"type": "episodic", "namespace": "agent-devis", "id": "...", "content": "...", "created_at": "..."}
{"type": "semantic", "namespace": "agent-devis", "key": "client.dupont", "value": "...", "ttl": null}
{"type": "procedural", "namespace": "agent-devis", "name": "generate_quote", "steps": [...]}
```

La première ligne est toujours l'en-tête de version. Les lignes suivantes sont les entrées dans n'importe quel ordre.

### Migration de schéma

Le champ `version` dans l'en-tête permet la migration forward-compatible :
- `version: 1` → schéma V1 actuel
- Si `version > version_courante` → erreur explicite avec le numéro de version

La migration inverse (import d'un fichier V2 dans une installation V1) retourne une erreur claire au lieu de corrompre la base.

### Commandes CLI

```bash
# Export de toute la mémoire
apollia-os memory export --output ~/backup/memory.jsonl.gz

# Export d'un namespace spécifique
apollia-os memory export --namespace agent-devis

# Import (merge - pas de remplacement)
apollia-os memory import ~/backup/memory.jsonl.gz

# Import avec remplacement (destructif - demande confirmation)
apollia-os memory import --replace ~/backup/memory.jsonl.gz
```

### Import = Merge par défaut

L'import fusionne les données importées avec les données existantes. Les conflits (même `id` épisodique) sont résolus par `created_at` - la plus récente gagne. Ce comportement est documenté.

---

## Conséquences

**Positives :**
- JSONL : lisible par n'importe quel outil (`zcat | jq`) - pas de format propriétaire
- Gzip : ratio de compression ~10× sur les données textuelles (typique pour la mémoire d'agent)
- Migration de schéma versionnée : les imports futurs resteront compatibles

**Négatives / Compromis :**
- JSONL ne préserve pas les index FTS5 - l'import reconstruit l'index. Sur les grandes bases (>100K entrées), la reconstruction peut prendre quelques secondes.
- Pas de chiffrement de l'export - les fichiers exportés contiennent des données potentiellement sensibles. L'utilisateur est responsable de la sécurité du fichier d'export.

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : L'export est un fichier local. Pas d'upload automatique. Conforme.
- **Principe #4 - Fail fast** : Version incompatible → erreur immédiate avec message clair. Conforme.

---

## Liens

- Story d'implémentation : STORY-484 (Sprint 37)
- Implémenté dans : `crates/apollia-memory/src/export.rs`
