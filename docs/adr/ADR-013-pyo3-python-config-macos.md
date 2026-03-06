# ADR-013 — Configuration PyO3 Python sur macOS via PYO3_PYTHON

**Date :** 2026-03-06
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 4

---

## Contexte

Le Sprint 4 introduit `apollia-aip`, la crate PyO3 qui charge et exécute des agents Python dans le runtime Rust. Sur macOS, le Python system (`/usr/bin/python3`) pointe vers le framework Xcode/CommandLineTools (Python 3.9). Le linker PyO3 cherche `libpython3.9` sous `/Applications/Xcode.app/Contents/Developer/Library/Frameworks/Python3.framework/Versions/3.9/lib`, un chemin qui n'existe pas quand seuls les CommandLineTools sont installés (le vrai chemin est sous `/Library/Developer/CommandLineTools/...`).

Ce mismatch provoque un échec de link systématique :
```
ld: library 'python3.9' not found
```

Sur Linux, le Python system fonctionne directement avec PyO3 sans configuration supplémentaire.

## Décision

Nous utilisons la variable d'environnement `PYO3_PYTHON` pour pointer vers un Python Homebrew (3.12+) sur macOS. Le `.cargo/config.toml` du workspace ne force PAS cette variable (trop spécifique à la machine). La documentation du projet mentionne le prérequis.

```bash
# macOS dev setup
export PYO3_PYTHON=/opt/homebrew/bin/python3.13
cargo test -p apollia-aip
```

Sur Linux (CI et production), aucune configuration supplémentaire n'est nécessaire.

## Alternatives considérées

### Option A — Forcer PYO3_PYTHON dans .cargo/config.toml (rejetée)
**Pour :** Configuration automatique pour tous les développeurs macOS.
**Contre :** Le chemin Homebrew varie selon la version Python installée et l'architecture (Intel vs ARM). Un chemin en dur casserait sur d'autres machines. Viole la portabilité.

### Option B — Exiger Xcode.app complet au lieu de CommandLineTools (rejetée)
**Pour :** Le chemin Python serait correct.
**Contre :** Xcode.app pèse 12+ GB. Disproportionné pour un problème de link Python. Non justifiable pour les contributeurs.

### Option C — Pinner PyO3 à une version qui résout automatiquement le chemin (rejetée)
**Pour :** Zéro configuration.
**Contre :** PyO3 0.22 délègue la résolution à `python3-config` qui retourne un chemin incorrect avec le Python system macOS. Ce n'est pas un bug PyO3 mais une spécificité CommandLineTools.

### Option retenue — PYO3_PYTHON vers Homebrew Python
**Pour :** Simple, explicite, fonctionne sur toutes les machines macOS avec Homebrew. Compatible avec le Principe #8 (CLI humaine).
**Compromis acceptés :** Le développeur doit installer Python via Homebrew et configurer une variable d'environnement.

## Conséquences

**Positives :**
- Fonctionne immédiatement sur Linux (CI) sans configuration
- Le développeur macOS contrôle quelle version Python est utilisée
- Compatible avec toutes les versions Python supportées par PyO3 0.22 (3.7+)

**Négatives / Compromis :**
- Étape de setup supplémentaire pour les contributeurs macOS
- Erreur de link peu explicite si PYO3_PYTHON n'est pas configuré

**Neutres / À surveiller :**
- PyO3 0.23+ pourrait améliorer la détection automatique sur macOS
- Si le projet adopte un Makefile/justfile, y inclure la vérification PYO3_PYTHON

## Principes architecturaux impactés

- Principe #2 — Zéro dépendance externe : Homebrew Python est une dépendance de développement, pas de production. Le binaire compilé embarque le runtime Python via PyO3. Pas de violation.

## Liens

- Story associée : STORY-024
- PyO3 configuration : https://pyo3.rs/v0.22/building-and-distribution#configuring-the-python-version
- ADR-012 : Précédent similaire (comportement différencié macOS/Linux)
