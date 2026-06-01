# ADR-064 - OAuth2 PKCE : Keyring Multi-Plateforme vs Fichier Chiffré

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 37 (planifié)

---

## Contexte

Apollia OS doit stocker des tokens OAuth2 (refresh tokens) pour les intégrations nécessitant une authentification tierce (Google Calendar, GitHub, Notion, etc.). Ces tokens sont des secrets à vie longue - leur compromission donne un accès persistant aux services de l'utilisateur.

**Options évaluées :**
1. **Keyring système** (`keyring` crate Rust) - délègue au gestionnaire de secrets natif de l'OS
2. **Fichier chiffré** - chiffrement AES-256-GCM avec une clé dérivée du mot de passe utilisateur
3. **Variables d'environnement** - pattern existant pour les API keys statiques

---

## Décision

**Choix : Keyring système multi-plateforme via la crate `keyring`.**

| Plateforme | Backend |
|-----------|---------|
| macOS | Keychain Services |
| Linux | Secret Service (GNOME Keyring / KDE Wallet) |
| Windows | Windows Credential Manager |

```rust
use keyring::Entry;

let entry = Entry::new("apollia-os", &format!("oauth2:{service}:{user_id}"))?;
entry.set_password(&token_json)?;
let token_json = entry.get_password()?;
```

### Rejet du fichier chiffré

Un fichier chiffré (ex. `~/.apollia/secrets.enc`) nécessite :
1. Une clé de dérivation - soit hardcodée (faible), soit demandée à l'utilisateur à chaque démarrage (friction), soit stockée dans... le keyring (circularité)
2. La gestion des migrations de schéma de chiffrement
3. Un vecteur d'initialisation (IV) par entrée + authentification MAC

La gestion correcte du chiffrement de fichiers est complexe et error-prone. Le keyring système est audité et maintenu par l'OS - meilleure garantie de sécurité pour le même effort d'implémentation.

### Rejet des variables d'environnement pour OAuth2

Les variables d'environnement sont adaptées aux API keys statiques (longue durée, rotation manuelle). Elles sont inadaptées aux tokens OAuth2 car :
- Les refresh tokens expirent et nécessitent une rotation automatique
- Le processus doit pouvoir écrire les nouveaux tokens - impossible avec des env vars héritées

---

## Conséquences

**Positives :**
- Sécurité OS-native - les tokens sont protégés par le même mécanisme que les mots de passe du navigateur
- Zéro gestion de clé de chiffrement côté Apollia
- Multi-plateforme sans code conditionnel dans Apollia (la crate `keyring` abstrait les backends)

**Négatives / Compromis :**
- Sur Linux sans GNOME Keyring / KDE Wallet installé : `keyring` échoue → fallback sur fichier `.apollia/tokens.json` non chiffré avec `chmod 600` + warning explicite dans les logs
- Les backups du `~/.apollia/` n'incluent pas les tokens (dans le keyring) → comportement attendu mais surprenant pour les utilisateurs

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : Les tokens sont stockés localement dans le keyring système. Conforme.
- **Principe #2 - Zéro dépendance externe** : La crate `keyring` compile sans C externe sur macOS/Windows. Sur Linux, elle utilise D-Bus (disponible sur tout desktop Linux). Conforme.

---

## Liens

- Story d'implémentation : STORY-478 (Sprint 37)
- Implémenté dans : `crates/apollia-core/src/secrets.rs`
