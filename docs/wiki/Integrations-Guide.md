# Guide Intégrations

> Comment découvrir, configurer et gérer des connexions MCP depuis l'interface desktop d'Apollia OS.
> Public cible : utilisateur Apollia (mode operator ou builder) qui veut connecter des outils externes sans toucher à `mcp.toml`.

---

## 1. La page Intégrations

La page Intégrations est accessible depuis la barre de navigation latérale. Son rendu change selon le mode actif :

| Mode | Libellé dans l'UI | Contenu principal |
|---|---|---|
| **Operator** | Connexions | Cards de connexion + catalogue de connecteurs |
| **Builder** | MCP Servers | Liste des serveurs avec PID/uptime/outils + registry browser |

Pour changer de mode, utilisez le sélecteur de mode en haut à gauche de l'interface. Le rendu change instantanément sans rechargement.

---

## 2. Ajouter une connexion (mode Operator)

### 2.1 Naviguer vers Connexions

1. Ouvrez l'application Apollia OS.
2. Sélectionnez **mode Operator** dans le sélecteur de mode.
3. Cliquez sur **Intégrations** dans la barre de navigation.
4. La page affiche vos connexions actives et le catalogue des connecteurs disponibles.

### 2.2 Parcourir le catalogue

Le catalogue est alimenté par le MCP Registry officiel (`registry.modelcontextprotocol.io`). Il liste les serveurs MCP disponibles avec leur niveau de confiance, leur description et leur publisher.

En l'absence de connexion Internet, le catalogue affiche le **cache local** (`~/.apollia/cache/mcp-registry.json`) avec un bandeau indiquant que les données peuvent être datées.

Utilisez la barre de recherche pour filtrer par mot-clé (ex. : "notion", "sqlite", "github").

### 2.3 Sélectionner un connecteur

Cliquez sur la card d'un connecteur dans le catalogue. Un panneau de détail s'ouvre avec :
- La description du serveur et son publisher
- Le niveau de confiance (voir section [Niveaux de confiance](#41-niveaux-de-confiance))
- Les prérequis (Node.js, Python, etc.)
- Le bouton **Configurer** pour lancer le wizard

### 2.4 Suivre le wizard (5 étapes)

Le wizard guide la configuration en 5 étapes :

| Étape | Contenu |
|---|---|
| 1. Présentation | Description du connecteur, niveau de confiance, disclaimer de sécurité (première fois uniquement) |
| 2. Prérequis | Vérification que la commande (`npx`, `uvx`, etc.) est disponible sur la machine |
| 3. Paramètres | Champs de configuration spécifiques au connecteur (chemin de base de données, URL, etc.) |
| 4. Authentification | Saisie des tokens et clés API — stockés dans le keychain OS, jamais dans mcp.toml |
| 5. Test & Confirmation | Test de connexion en direct, liste des outils découverts, confirmation |

À la confirmation, le serveur démarre immédiatement et apparaît dans vos connexions.

---

## 3. Gérer les connexions (mode Operator)

### 3.1 Surveiller le statut

Chaque connexion active est représentée par une card avec :
- **Nom du serveur** et icône du connecteur
- **Indicateur de statut** : point vert (connecté), point rouge (erreur), point gris (déconnecté)
- **Nombre d'outils** découverts lors du handshake
- **Badge de confiance** (Official, Verified, Community, Custom)

### 3.2 Actions disponibles sur une connexion

Cliquez sur la card ou sur le menu `...` pour accéder aux actions :

| Action | Description |
|---|---|
| **Tester** | Lance un handshake éphémère pour vérifier que la connexion est encore valide |
| **Redémarrer** | Arrête le processus serveur et en démarre un nouveau avec la même configuration |
| **Modifier** | Ouvre le wizard en mode édition pour mettre à jour les paramètres ou les secrets |
| **Déconnecter** | Arrête le processus et supprime la configuration de `mcp.toml` |

> **Note :** La déconnexion supprime aussi les secrets associés du keychain OS. Cette action est irréversible — les tokens devront être re-saisis lors d'une reconnexion.

### 3.3 Niveaux d'approbation

Certains connecteurs sont configurés avec un niveau d'approbation qui impose une validation humaine avant chaque appel d'outil. Ce comportement est géré par le champ `requires_approval` dans la configuration MCP.

Quand `requires_approval` est actif, chaque appel d'outil du serveur est suspendu jusqu'à ce que vous approuviez ou refusiez l'action depuis le dashboard ou via l'API REST (`POST /api/v1/approvals/:id/approve`).

Pour les serveurs accédant à des données sensibles ou effectuant des actions irréversibles (suppression, envoi d'e-mail, publication), il est recommandé d'activer cette option.

---

## 4. Sécurité

### 4.1 Niveaux de confiance

Chaque serveur MCP dans le catalogue est étiqueté avec un niveau de confiance selon la provenance de son code et l'identité de son publisher :

| Niveau | Badge | Couleur | Signification |
|---|---|---|---|
| `verified_official` | Official | Vert | Serveur maintenu par l'organisation officielle du service (ex. : Notion Inc. pour Notion MCP) |
| `community_verified` | Verified | Bleu | Serveur tiers audité et vérifié par l'équipe MCP Registry |
| `community` | Community | Jaune | Serveur communautaire publié sans audit formel — à inspecter avant usage en production |
| `custom` | Custom | Gris | Serveur configuré manuellement, non répertorié dans le registry |

> Les niveaux `community` et `custom` ne signifient pas que le serveur est malveillant, mais que sa sécurité n'a pas été vérifiée par un tiers. Lisez le code source ou l'audit du publisher avant de le connecter à des données sensibles.

### 4.2 Stockage des secrets (OS Keychain)

Les tokens et clés API saisis dans le wizard ne sont **jamais écrits en clair** dans `mcp.toml`. Ils sont stockés dans le keychain natif de l'OS sous le service `apollia-mcp` :

| OS | Backend keychain |
|---|---|
| macOS | macOS Keychain (Keychain Access) |
| Linux | Secret Service via D-Bus (`org.freedesktop.secrets`) |
| Windows | Windows Credential Manager |

La clé utilisée dans le keychain suit le format `{server_name}:{env_var_name}` — par exemple `notion:NOTION_API_KEY`.

Dans `mcp.toml`, la valeur correspondante est écrite comme `${APOLLIA_SECRET:NOTION_API_KEY}`. Au démarrage du runtime, `resolve_env()` détecte ce préfixe et lit la valeur depuis le keychain.

**Vérification sur macOS :**

```bash
security find-generic-password -s "apollia-mcp" -a "notion:NOTION_API_KEY" -w
```

**Vérification sur Linux :**

```bash
secret-tool lookup service apollia-mcp username "notion:NOTION_API_KEY"
```

**Limitation Linux :** si le service D-Bus `org.freedesktop.secrets` est absent (containers, serveurs headless), le keychain est indisponible. Dans ce cas, un fichier chiffré local est utilisé en fallback — voir `~/.apollia/secrets.enc`.

### 4.3 Niveaux d'approbation

Les niveaux d'approbation contrôlent si un humain doit valider chaque appel d'outil d'un serveur MCP. Deux mécanismes coexistent :

**Au niveau serveur** (configuration MCP) :

```toml
[[servers]]
name              = "brave-search"
command           = "npx"
args              = ["-y", "@modelcontextprotocol/server-brave-search"]
requires_approval = true
```

Quand `requires_approval = true`, **tous** les outils de ce serveur nécessitent une approbation humaine.

**Au niveau agent** (manifest Python) :

```python
def manifest(self):
    return {
        "tools_requiring_approval": ["mcp:brave-search/brave_web_search"],
    }
```

Les deux mécanismes sont cumulatifs : si l'un ou l'autre est actif, l'approbation est requise.

### 4.4 Disclaimer de sécurité et consentement

Avant le **premier ajout** de connexion MCP, un dialog de sécurité s'affiche. Il rappelle que :

1. **Vos données vont transiter** — les outils MCP sont des processus tiers qui reçoivent des données depuis vos agents.
2. **Faites confiance au publisher** — vérifiez l'identité et la réputation du maintainer du serveur.
3. **Vous êtes responsable** — l'usage des API tierces (quotas, CGU, coûts) reste sous votre responsabilité.
4. **Votre contrôle** — vous pouvez inspecter l'historique des appels, imposer des approbations, ou déconnecter à tout moment.

Le consentement est enregistré localement dans le `localStorage` du frontend sous la clé `apollia-mcp-disclaimer-accepted`. Ce dialog ne s'affiche qu'**une seule fois** — pour le réafficher, videz le localStorage (DevTools → Application → Local Storage) ou réinitialisez les préférences dans les paramètres.

---

## 5. Mode Builder — Serveurs MCP

Le mode Builder est destiné aux développeurs d'agents qui ont besoin de contrôle fin sur les processus MCP actifs.

### 5.1 Liste des serveurs

La liste affiche tous les serveurs MCP démarrés par le runtime, qu'ils aient été ajoutés via le wizard ou configurés manuellement dans `mcp.toml`. Chaque ligne indique :

- **Nom** du serveur
- **Statut** (connecté / erreur / déconnecté)
- **PID** du processus
- **Uptime** depuis le démarrage
- **Nombre d'outils** découverts
- **Badge de confiance**

### 5.2 Détail d'un serveur

Cliquez sur une ligne pour ouvrir le panneau de détail. Il contient trois onglets :

| Onglet | Contenu |
|---|---|
| **Outils** | Liste complète des outils MCP exposés par le serveur avec leur schéma d'entrée |
| **Logs** | Flux de logs du processus serveur en temps réel (stdout/stderr) |
| **Config** | Configuration brute du serveur (secrets redactés) et boutons Redémarrer / Supprimer |

### 5.3 Registry browser

Le registry browser (onglet ou panneau dédié en mode Builder) permet de rechercher des serveurs MCP dans le catalogue officiel sans lancer le wizard. Il est utile pour :

- Trouver le package npm/pip exact à installer
- Consulter les paramètres requis avant d'éditer `mcp.toml` manuellement
- Comparer plusieurs serveurs pour un même besoin

En mode offline, le browser affiche le cache local avec un indicateur de date de dernière mise à jour.

### 5.4 Édition manuelle de mcp.toml

Pour les utilisateurs avancés, `mcp.toml` peut toujours être édité directement dans `~/.apollia/mcp.toml`. Les modifications sont rechargées à chaud sans redémarrer le runtime.

Référence complète du format : [MCP — Guide utilisateur](./MCP-Guide-Utilisateur).

Pour utiliser un secret stocké dans le keychain plutôt qu'une variable d'environnement shell :

```toml
[[servers]]
name    = "notion"
command = "npx"
args    = ["-y", "@notionhq/notion-mcp-server"]

[servers.env]
NOTION_API_KEY = "${APOLLIA_SECRET:NOTION_API_KEY}"
```

Le runtime résout `APOLLIA_SECRET:NOTION_API_KEY` depuis le keychain OS au démarrage.

---

## 6. Troubleshooting

### 6.1 Le catalogue affiche le cache (mode offline)

**Cause :** le registry `registry.modelcontextprotocol.io` est injoignable (pas de connexion Internet, timeout 15 s).

**Action :** le cache local est utilisé automatiquement. Pour forcer une mise à jour, rétablissez la connexion et cliquez sur l'icône de rafraîchissement dans le catalogue.

### 6.2 Le wizard échoue à l'étape "Prérequis"

**Cause :** la commande de lancement (`npx`, `uvx`) n'est pas disponible dans le `PATH`.

**Action :**

```bash
which npx   # doit retourner un chemin
which uvx   # doit retourner un chemin
```

Si la commande est absente, installez Node.js (pour `npx`) ou `uv` (pour `uvx`) — voir [MCP — Guide utilisateur §2](./MCP-Guide-Utilisateur#2-prérequis).

### 6.3 Le test de connexion échoue à l'étape 5

**Cause fréquente :** token API invalide ou absent.

**Action :** vérifiez que le secret est bien présent dans le keychain (commandes §4.2). Si absent, revenez à l'étape Authentification du wizard et re-saisissez le token.

### 6.4 Le disclaimer ne réapparaît pas

**Cause :** le consentement a été enregistré dans `localStorage`.

**Pour réinitialiser :**

```
DevTools (F12) → Application → Local Storage → apollia-mcp-disclaimer-accepted → Supprimer
```

---

## Voir aussi

- [MCP — Guide utilisateur](./MCP-Guide-Utilisateur) — configuration manuelle via `mcp.toml`, référence des champs, API REST
- [Sécurité — Local-First](./Securite-Local-First) — principes de souveraineté des données
- [Briques Desktop](./Briques-Desktop) — architecture de l'application desktop Tauri
- [API-HTTP-Observability](./API-HTTP-Observability#mcp-sprint-26-adr-044) — routes REST MCP (`/api/v1/mcp/`)
