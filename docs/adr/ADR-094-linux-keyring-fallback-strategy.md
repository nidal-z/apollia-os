# ADR-094 - Linux keyring fallback strategy

**Date :** 2026-05-12
**Statut :** Proposé - décision à finaliser **avant le premier commit M1**
**Sprint :** Pré-implémentation (chantier Connecteurs & MCP v0.1.0)

---

## Contexte

`apollia-auth` stocke les access tokens + refresh tokens + secrets divers dans le keyring OS (macOS Keychain, Windows Credential Manager, Linux Secret Service via libsecret/D-Bus).

Sur Linux **headless** (serveurs sans session graphique, conteneurs, distros minimales sans gnome-keyring) : pas de daemon Secret Service → le crate `keyring` Rust échoue à l'initialisation. État actuel : non-géré → crash boot d'Apollia.

Pour la cible power user v0.1.0 (qui inclut explicitement les Linux server headless utilisateurs), il faut un **fallback chiffré sur disque**. Deux approches en concurrence.

## Décision

**À finaliser avant le premier commit M1.** Choix entre Option A et Option B ci-dessous. Décision provisoire : **Option A (age symétrique avec passphrase user)** - à confirmer après prototype rapide.

## Alternatives considérées

### Option A - `age` symétrique avec passphrase user (provisoirement retenue)
**Pour :**
- Zéro dépendance système (`age` est pure Rust, lib `rage`).
- Fonctionne identiquement sur tout Linux (server, container, desktop sans keyring).
- Audit cryptographique solide (age est bien revu).
- Storage simple : fichier `~/.apollia/secrets.age`.

**Contre :**
- Exige une passphrase à saisir au démarrage du runtime (mais : caching session via `apollia-runtime` acteur dédié possible, prompt une fois par session).
- Si l'utilisateur oublie la passphrase, tokens perdus (mitigation : la passphrase est OPTIONNELLE - si vide, on bascule sur chiffrement symétrique avec clé dérivée du user UID + machine-id ; faible mais > rien).

### Option B - `system-keyring-with-prompt` D-Bus user session
**Pour :**
- Pas de passphrase utilisateur.
- Cohérent avec macOS/Windows (toujours keyring).

**Contre :**
- Échoue silencieusement sur distros minimales sans D-Bus user session.
- Setup différent par distro (Ubuntu Server, Alpine, Debian minimal…). Dette doc + support.
- Solution "ça marche peut-être", anti fail-fast (principe #4).

### Option C - Fichier en clair (rejetée)
**Pour :** trivial.
**Contre :** **inacceptable** (tokens OAuth en clair sur disque = vulnérabilité critique).

## Conséquences

**Positives (Option A retenue) :**
- Fonctionne sur 100% des Linux (server, container, headless).
- Crypto auditée (`age` / `rage`).
- Activation via `APOLLIA_TOKEN_STORAGE=file` + `APOLLIA_TOKEN_PASSPHRASE=...` (env var optionnelle).

**Négatives / Compromis :**
- UX passphrase à l'init du runtime (mitigation : prompt once + cache acteur).
- Documentation help dédiée requise (`gerer-les-tokens-oauth.md`).

**À surveiller :**
- Adoption power user Linux server : si retour utilisateur indique que la passphrase est trop friction, basculer sur Option B avec opt-in.

## Principes architecturaux impactés

- Principe #1 - Local-first : ✅ tokens chiffrés au repos sur disque local.
- Principe #2 - Zéro dépendance externe : ✅ `age` est pure Rust embarquée.
- Principe #4 - Fail fast : si passphrase incorrecte, erreur explicite immédiate au boot, pas de fallback silencieux.

## Liens

- ADR-064 - OAuth2 PKCE keyring (étendu)
- ADR-088 - Architecture hybride
- Plan : §5.5
