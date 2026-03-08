# Sécurité — Local-First — Apollia OS

> Comment Apollia OS garantit que les données utilisateur ne quittent jamais la machine, et ce que ça signifie concrètement.
> Public cible : responsable sécurité, DSI, développeur soucieux de la souveraineté

---

## Vue d'ensemble

Le principe Local-First (Principe #1) est la garantie fondamentale d'Apollia OS : **aucun octet de données utilisateur ne quitte la machine sans une action explicite du développeur**.

Cette garantie n'est pas contractuelle ou de confiance — elle est architecturale. Il n'existe pas de serveur distant vers lequel Apollia OS pourrait envoyer des données, même accidentellement.

---

## Ce que "local-first" signifie concrètement

### Zéro connexion sortante par défaut

Le runtime Apollia OS n'ouvre aucune connexion réseau sortante par défaut :
- Pas de telemetry
- Pas de "phone home"
- Pas de vérification de licence
- Pas de téléchargement automatique de modèles

Vous pouvez vérifier :
```bash
# Surveiller les connexions réseau du runtime
ss -tnp | grep apollia-os
# Résultat attendu : aucune connexion

# Ou avec strace
strace -e trace=network -p $(pgrep apollia-os)
# Seules des opérations sur socket Unix et TCP loopback
```

### Mémoire persistante locale

Toute la mémoire persistante des agents est stockée dans un fichier SQLite local :
```bash
ls -lh /var/lib/apollia/memory.db
# Fichier local, jamais transmis

# Inspecter directement le contenu
sqlite3 /var/lib/apollia/memory.db "SELECT content FROM episodes LIMIT 5;"
```

### Audit trail local

Chaque appel d'outil est enregistré localement dans le même SQLite :
```bash
apollia-os audit list
# Tout en local, consultable et exportable à volonté
```

---

## Ce que les agents peuvent faire (et ne pas faire)

### Accès réseau des agents : opt-in explicite

Par défaut, un agent ne peut pas faire d'appels réseau sortants depuis ses outils :

```python
def manifest(self):
    return {
        "network_allowlist": None,  # None = aucun accès réseau
    }
```

Pour autoriser des appels réseau spécifiques, le développeur doit l'expliciter :
```python
def manifest(self):
    return {
        "network_allowlist": ["api.openai.com", "api.anthropic.com"],
    }
```

Cette whitelist est appliquée au niveau du sandbox (`unshare`) — l'agent ne peut pas la contourner depuis son code Python.

### Outils dangereux : opt-in explicite

Les outils marqués `dangerous=true` dans le Tool Registry (ex: `bash_executor` sans sandbox) sont bloqués par défaut :

```python
def manifest(self):
    return {
        "dangerous_tools_allowed": False,  # défaut
    }
```

Un opérateur qui déploie un agent avec `dangerous_tools_allowed: True` fait un choix explicite et conscient.

---

## Flux de données — ce qui reste local

```
Agent Python
    │
    │ ctx.tools.call("file_io", {...})
    ▼
ToolProxy (apollia-aip)
    │
    │ execute()
    ▼
FileIo / BashExecutor / PythonExecutor
    │
    │ résultat JSON
    ▼
Agent Python
    │
    │ ctx.memory.record(...)
    ▼
MemoryManager (apollia-memory)
    │
    │ INSERT INTO episodes ...
    ▼
SQLite local (/var/lib/apollia/memory.db)
```

À aucun moment dans ce flux des données ne sortent de la machine.

---

## Ce qui PEUT sortir (avec action explicite)

Apollia OS ne peut pas contrôler le code Python de l'agent. Si un agent appelle `requests.get("https://api.externe.com", data=donnees_sensibles)`, les données sortent.

**La responsabilité de l'opérateur :**
1. Auditer le code source des agents déployés
2. Utiliser `network_allowlist` pour restreindre les domaines accessibles
3. Activer le sandbox pour isoler les outils système
4. Consulter l'audit trail régulièrement

**Ce qu'Apollia OS garantit :**
- Le runtime lui-même n'exfiltre rien
- Les outils natifs respectent les contraintes sandbox
- L'audit trail enregistre chaque appel d'outil (traçabilité)

---

## Conformité RGPD / souveraineté

Apollia OS est conçu pour les contextes où la souveraineté des données est non-négociable :
- Secteur santé (données patients)
- Finance (données financières)
- Défense / secteur public
- PME avec contraintes contractuelles clients

**Checklist conformité :**
- ✅ Zéro cloud dans le chemin de traitement
- ✅ Données stockées localement dans SQLite
- ✅ Audit trail de tous les accès outils
- ✅ Isolation des agents via namespaces
- ✅ Accès réseau opt-in uniquement
- ⚠️ Le code Python de l'agent est sous la responsabilité du développeur
- ⚠️ Les modèles LLM appelés par l'agent (OpenAI, Anthropic...) traitent les données hors machine

---

## Voir aussi

- [Architecture Principes](./Architecture-Principes) — Principe #1 Local-first
- [Sécurité Sandbox Isolation](./Securite-Sandbox-Isolation) — Linux namespaces
- [Sécurité Guardrails](./Securite-Guardrails) — StepBudget et circuit breakers
- [ADR-010](../adr/ADR-010-pivot-saas-vers-runtime-rust-open-source) — contexte du pivot vers local-first
