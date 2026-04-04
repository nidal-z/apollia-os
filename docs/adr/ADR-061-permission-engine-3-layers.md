# ADR-061 — Permission Engine 3 Couches

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 36 (planifié)

---

## Contexte

Le moteur de permission actuel d'Apollia OS repose sur une vérification binaire (`dangerous: bool` dans `ToolDescriptor`) et un profil sandbox prédéfini. Cette approche est suffisante pour les outils natifs, mais insuffisante pour le moteur de permission des commandes shell arbitraires (notamment `BashExecutor` et `PersistentBashExecutor`).

**Problème identifié :** les outils bash peuvent exécuter des commandes arbitrairement dangereuses (`rm -rf /`, `curl | bash`, injections de commandes via les arguments). Sans couche de permission granulaire, le seul garde-fou est le sandbox Linux (namespaces), qui peut être insuffisant si l'agent s'échappe du sandbox.

**Standards de référence :**
- OWASP ASVS V1.4 — Access Control Architecture
- NIST SP 800-190 — Container Security
- CWE-269 — Improper Privilege Management
- CWE-400 — Uncontrolled Resource Consumption
- POSIX Shell Grammar + ShellCheck AST

---

## Décision

**Architecture 3 couches en cascade :**

```
Commande shell entrante
  │
  ▼
Couche 1 : SafeList
  │  Liste blanche configurée par l'opérateur dans apollia.toml
  │  Si la commande est dans la SafeList → autorisée directement
  │  SafeList vide par défaut (OWASP ASVS V1.4 : deny by default)
  │
  ▼ (si pas dans SafeList)
Couche 2 : RiskClassifier
  │  Classification en 4 catégories de risque selon OWASP A10 / NIST SP 800-190
  │  SAFE → autorisé
  │  LOW_RISK → autorisé avec log audit
  │  HIGH_RISK → bloqué si block_high_risk = true (défaut : true)
  │  CRITICAL → toujours bloqué
  │
  ▼ (si SAFE ou LOW_RISK)
Couche 3 : StructuralInjectionDetector
     Analyse AST POSIX/ShellCheck des arguments
     Détecte les tentatives d'injection (command substitution, redirections imbriquées)
     Si injection détectée → bloqué + audit + warn!
```

**Configuration `apollia.toml` :**

```toml
[permissions]
safe_commands = []          # SafeList — vide par défaut (deny by default)
block_high_risk = true      # Bloquer les commandes HIGH_RISK
block_network_in_bash = true  # Interdire curl/wget/nc dans BashExecutor
```

### Rejet de la liste hardcodée BANNED_COMMANDS

Une approche alternative est de maintenir une liste hardcodée de commandes bannies (`rm -rf`, `mkfs`, `dd if=/dev/zero`, etc.). Cette approche est rejetée car :
1. Non configurable — les opérateurs légitimes (ex. agent de nettoyage de disque) ne peuvent pas l'adapter
2. Non maintenable — la liste est infinie et devient obsolète à chaque nouvel outil dangereux
3. Contournable via alias, scripts, ou encodage unicode

Le `RiskClassifier` basé sur des patterns sémantiques est plus robuste et extensible.

---

## Conséquences

**Positives :**
- SafeList vide par défaut : conforme OWASP ASVS V1.4 (deny by default, allow by exception)
- L'opérateur peut déclarer exactement les commandes autorisées pour son déploiement
- Le `StructuralInjectionDetector` protège contre les injections dans les arguments (vecteur principal d'attaque)

**Négatives / Compromis :**
- Overhead d'analyse AST sur chaque commande — acceptable sur les fréquences d'appel typiques (<100/s)
- Le `RiskClassifier` peut produire des faux positifs sur des commandes légitimes complexes — la SafeList permet de les exclure explicitement

---

## Principes architecturaux impactés

- **Principe #4 — Fail fast** : Les commandes CRITICAL sont rejetées avant toute exécution. Conforme.
- **Principe #2 — Zéro dépendance externe** : ShellCheck est une bibliothèque Rust (`shellcheck-rs`) ou une analyse via regex — pas de binaire externe requis. Conforme.

---

## Liens

- Stories d'implémentation : STORY-466, STORY-467, STORY-490 (Sprint 36)
- Implémenté dans : `crates/apollia-tools/src/permission/`
