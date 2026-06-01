# ADR-051 - Authentification API REST TCP : token statique + restriction loopback

**Date :** 2026-04-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 34 - Beta Hardening

---

## Contexte

L'API REST TCP `:7771` est ouverte sans aucune authentification depuis le Sprint 5. Toute application
locale peut appeler `/api/v1/agents/start`, `/api/v1/memory/clear`, ou n'importe quel endpoint
destructif sans preuve d'identité. Sur macOS et Linux, n'importe quel processus de l'utilisateur
courant peut atteindre ce port.

Avant d'entrer en beta publique, ce vecteur doit être fermé. Deux surfaces d'accès coexistent :

- **Socket Unix** (`~/.apollia/runtime.sock`) - accès par permissions filesystem, réservé aux
  processus sous le même UID. Pas d'authentification supplémentaire requise.
- **TCP `:7771`** - liaison réseau, accessible à tous les processus locaux et, si mal configuré,
  au réseau local. C'est la surface à sécuriser.

Le Principe #1 (Local-first) exclut tout mécanisme nécessitant un serveur distant pour la
vérification. Le Principe #4 (Fail fast) impose que le token soit validé au démarrage et que toute
requête sans token valide soit rejetée immédiatement avec `401 Unauthorized`.

---

## Décision

### 1. Token statique 32 octets hexadécimaux

Un token de 256 bits (32 octets aléatoires encodés en hexadécimal, soit 64 caractères) est généré
au premier démarrage du runtime via `rand::rngs::OsRng`.

Le token est stocké dans `~/.apollia/api-token` avec permissions `0600` (lecture/écriture
propriétaire uniquement). Le runtime refuse de démarrer si le fichier est lisible en groupe ou en
world (`0640`, `0644`, etc.) - Principe #4.

```
~/.apollia/api-token   # 64 hex chars + newline, chmod 0600
```

### 2. En-tête Authorization Bearer

Toutes les requêtes TCP doivent inclure :

```http
Authorization: Bearer <hex-token>
```

Le middleware axum `auth_middleware` intercepte chaque requête avant les handlers. Il retourne
`401 Unauthorized` si l'en-tête est absent ou si le token ne correspond pas (comparaison à
temps constant via `subtle::ConstantTimeEq` pour éviter les timing attacks).

### 3. Restriction loopback par défaut

TCP `:7771` est bindé sur `127.0.0.1` par défaut - jamais sur `0.0.0.0`. La config expose
`api.bind` pour les cas avancés (VMs, conteneurs) mais la documentation avertit explicitement
des risques d'exposition réseau.

### 4. Socket Unix non authentifiée

Le socket Unix conserve son comportement actuel : permissions filesystem (`srwxr-x---`) suffisent.
L'app desktop Tauri utilise exclusivement le socket Unix et n'est pas affectée.

### 5. Rotation de token

Aucune rotation automatique. Rotation manuelle via `apollia-os config rotate-token` qui régénère
le fichier et redémarre le runtime. Pour la beta, cette simplicité est suffisante.

### 6. Propagation aux clients

La CLI lit le token depuis `~/.apollia/api-token` automatiquement. Les agents Python accèdent
au runtime via socket Unix et ne sont pas concernés. Les clients externes (scripts, outils tiers)
doivent lire le token manuellement.

---

## Alternatives considérées

| Option | Raison du rejet |
|---|---|
| **OAuth2 / JWT** | Trop complexe pour une beta locale mono-utilisateur. Nécessite un serveur d'autorisation ou une clé secrète partagée avec complexité de renouvellement. |
| **mTLS (mutual TLS)** | Overhead d'infrastructure (CA locale, certificats clients). Aucun avantage sur le token statique pour un contexte mono-utilisateur. |
| **Aucune authentification** | Vecteur d'attaque par toute application locale (XSS via navigateur sur localhost, autres processus malveillants). Inacceptable pour beta publique. |
| **Token par session** | Complexité de gestion de session non justifiée. Le runtime est un processus singleton - pas de multi-session à gérer. |
| **Restriction par PID/UID** | Dépend des APIs OS non portables. Ne fonctionne pas pour les clients réseau légitimes (tests, intégrations CI). |

---

## Conséquences

**Positives :**
- Sécurité locale suffisante pour une beta mono-utilisateur sans infrastructure externe.
- Comparaison à temps constant → pas de timing attack sur le token.
- Backward-compatible : les clients existants via socket Unix ne sont pas affectés.
- Simple à auditer - 64 hex chars dans un fichier, un middleware axum, un en-tête HTTP.

**Négatives / Compromis :**
- Pas de rotation automatique - un token compromis persiste jusqu'à intervention manuelle.
- Les clients TCP existants (scripts de test, outils tiers) doivent être mis à jour pour inclure
  l'en-tête `Authorization`.
- Si le fichier `~/.apollia/api-token` est copié/volé avec la config, le token est compromis.

**Neutres / À surveiller :**
- Pour une version multi-utilisateur (post-v1), un système de scopes par token sera nécessaire.
- Le token n'expire pas - à documenter explicitement dans la page de sécurité du wiki.

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : Zéro endpoint distant pour la vérification du token. Le fichier
  `~/.apollia/api-token` est la source de vérité, stockée sur disque local. Conforme.
- **Principe #4 - Fail fast** : Permissions incorrectes sur `api-token` → erreur au démarrage,
  pas en cours d'exécution. Token absent → erreur au démarrage. Requête sans token → `401`
  immédiat. Renforcé.
- **Principe #8 - CLI humaine, API machine** : `apollia-os config show-token` affiche le token
  en clair pour les intégrations. `apollia-os config rotate-token` pour la rotation. Conforme.

---

## Liens

- Story d'implémentation : STORY-428 (Sprint 34)
- Middleware implémenté dans : `crates/apollia-runtime/src/api/middleware.rs`
- Config exposée dans : `crates/apollia-core/src/config.rs` - section `[api]`
- ADR observabilité (loopback) : [ADR-006](ADR-006-rest-json-api-locale.md)
