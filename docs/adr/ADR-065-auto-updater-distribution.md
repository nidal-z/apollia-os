# ADR-065 - Auto-Updater : Binaire Direct + SHA256

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 37 (planifié)

---

## Contexte

Apollia OS doit proposer un mécanisme de mise à jour automatique pour les utilisateurs qui ont installé le binaire directement (sans package manager). Les canaux de distribution actuels sont :
- Binaire direct (`curl | install.sh`)
- GitHub Releases

Sans auto-updater, les utilisateurs restent sur des versions obsolètes et n'obtiennent pas les correctifs de sécurité.

**Options évaluées :**
1. **Binaire direct + SHA256** - vérification d'intégrité du binaire téléchargé
2. **Package manager** (Homebrew, apt, cargo install) - délègue au système de paquets
3. **Auto-update via `cargo install`** - recompilation sur la machine cible

---

## Décision

**Choix : Binaire direct + vérification SHA256 + lock file.**

**Mécanisme :**
1. `apollia-os update check` → télécharge `VERSION` depuis GitHub Releases
2. Si version > version courante → propose la mise à jour
3. `apollia-os update apply` :
   a. Télécharge le binaire pour l'architecture cible
   b. Télécharge `SHA256SUMS` et vérifie l'intégrité
   c. Écrit le lock file `~/.apollia/update.lock` (empêche les mises à jour concurrentes)
   d. Remplace atomiquement le binaire courant (`rename(tmp, current_binary)`)
   e. Supprime le lock file

```
https://github.com/apollia-os/apollia-os/releases/latest/download/
  apollia-os-x86_64-unknown-linux-musl
  apollia-os-aarch64-apple-darwin
  SHA256SUMS
```

### Rejet du package manager

Les package managers (Homebrew, apt) sont délégués à des infrastructures tierces et requièrent des releases publiées dans des registres séparés. Ils ajoutent de la complexité opérationnelle disproportionnée pour la phase beta.

La distribution binaire directe (pattern Ollama, Tauri, Helix) est suffisante pour la beta et compatible avec la roadmap future (packages officiels en v1.0).

### Rejet de `cargo install`

`cargo install` recompile le code source sur la machine cible. Cela requiert Rust installé et prend > 5 minutes. Inacceptable pour une mise à jour utilisateur.

---

## Conséquences

**Positives :**
- SHA256 : protection contre la corruption de téléchargement et les attaques MITM basiques
- Lock file : protection contre les mises à jour concurrentes (ex. deux terminaux ouverts)
- Remplacement atomique (`rename`) : pas de binaire corrompu en cas d'interruption

**Négatives / Compromis :**
- Pas de signature GPG en V1 - SHA256 seul ne protège pas contre un serveur GitHub compromis. Ajout de la vérification de signature dans un sprint futur.
- Le binaire musl (static) est plus gros que les binaires dynamiques - acceptable (distribution unique, pas de package manager à gérer)

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : La mise à jour est explicitement déclenchée par l'utilisateur (`apollia-os update apply`). Aucune mise à jour silencieuse en arrière-plan. Conforme.
- **Principe #4 - Fail fast** : Si SHA256 ne correspond pas → erreur explicite avec le hash attendu vs reçu, binaire non installé. Conforme.

---

## Liens

- Story d'implémentation : STORY-479 (Sprint 37)
- Implémenté dans : `crates/apollia-cli/src/commands/update.rs`
