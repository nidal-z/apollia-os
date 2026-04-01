# ADR-050 — Distribution des Worker Agents : bundled vs communautaire, registre local et Git

**Date :** 2026-04-01
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 32 (Pré-implémentation)

---

## Contexte

Les Sprints 29-31 livrent quatre Worker Agents (`excel-worker`, `csv-data-worker`, `pdf-worker`,
`code-worker`) mais sans packaging ni séparation formalisée. Tous les agents résident dans `agents/`
à plat, sans distinction entre ceux distribués avec le runtime et ceux installés par l'utilisateur.

Trois questions architecturales doivent être tranchées avant d'implémenter le packaging (STORY-410)
et le registre communautaire (STORY-411) :

1. **Quels agents sont bundled ?** — distribués avec le binaire Apollia, installés automatiquement
2. **Format du registre communautaire** — comment un tiers déclare et distribue un agent
3. **Commande d'installation** — contrat d'interface `apollia-os agent install`
4. **Validation à l'installation** — ce qui est vérifié avant qu'un agent communautaire soit accepté
5. **Séparation physique** — arborescence `agents/bundled/` vs `agents/community/`
6. **Auto-installation des bundled** — moment et mécanisme du premier déploiement

Le Principe #2 (Zéro dépendance externe) contraint fortement ces décisions : le runtime doit
fonctionner seul, sans endpoint distant obligatoire pour obtenir les agents bundled. Le Principe #4
(Fail fast) impose que la validation d'un agent tiers se produise à l'installation, pas à l'exécution.

---

## Décision

### 1. Agents bundled — les quatre Worker Agents de domaine général

Les agents distribués avec le runtime Apollia (et installés automatiquement au premier boot) sont :

| Agent | Domaine | Sprint de création |
|---|---|---|
| `excel-worker` | Tableurs Excel (xlsx, xls) | Sprint 29 |
| `csv-data-worker` | Fichiers CSV et données tabulaires | Sprint 29 |
| `pdf-worker` | Documents PDF (extraction, analyse) | Sprint 31 |
| `code-worker` | Génération et refactoring de code | Sprint 31 |

Ces quatre agents couvrent les cas d'usage PME les plus fréquents. Ils sont testés, documentés,
et maintenus par l'équipe Apollia. Tout agent supplémentaire, y compris `sql-worker` et `git-worker`
(Sprint 32), est distribué comme agent communautaire.

**Critère d'inclusion bundled :** un agent est bundled si et seulement si (a) il est maintenu par
l'équipe Apollia, (b) il couvre un cas d'usage généraliste sans configuration spécifique à
l'infrastructure de l'utilisateur, et (c) ses dépendances pip sont stables et sans risque de
conflit (openpyxl, pandas, pdfplumber).

`sql-worker` et `git-worker` sont volontairement exclus des bundled : ils nécessitent une
configuration d'infrastructure spécifique (chemin de base de données, dépôt Git) et sont utilisés
dans des contextes plus contraints.

### 2. Format du registre communautaire — répertoire local V1, repo Git V2

**V1 (ce sprint) :** Le registre est un répertoire local `agents/community/` dans le projet
Apollia. Chaque agent communautaire est un sous-dossier contenant :

```
agents/community/<agent-name>/
├── agent.py          ← fichier agent principal (contrat AIP : manifest() + run())
├── manifest.json     ← copie du manifest déclaré par manifest()
└── README.md         ← description, usage, exemples
```

Le fichier `manifest.json` est le contrat formel. Il doit être cohérent avec ce que `manifest()`
retourne au runtime — toute divergence est détectée à l'installation (validation statique).

**V2 (post-Sprint 32) :** Le registre évolue vers un repo Git public hébergé séparément
(`apollia-os/community-registry`), avec un fichier d'index `registry.json` à la racine listant
les agents disponibles. Le runtime pourra résoudre une URL Git → cloner → valider → installer.
Aucun serveur central n'est requis — le repo Git est le registre.

### 3. Commande d'installation — `apollia-os agent install <source>`

```
# Installation depuis un path local (V1)
apollia-os agent install ./agents/community/sql-worker/

# Installation depuis une URL Git (V2)
apollia-os agent install https://github.com/org/my-worker.git
```

Le `<source>` est résolu dans l'ordre suivant :
1. Path absolu ou relatif → installation locale directe
2. URL Git (`https://` ou `git@`) → clone dans un répertoire temporaire, puis installation
3. Identifiant court (`sql-worker`) → lookup dans le registre communautaire configuré (V2)

La commande est synchrone et interactive : elle affiche les métadonnées de l'agent, les packages
pip requis, et demande confirmation avant d'écrire quoi que ce soit sur le disque.

### 4. Validation à l'installation

La validation est effectuée par le runtime Rust à l'appel de `apollia-os agent install`. Elle
comprend quatre étapes séquentielles — un échec à n'importe quelle étape interrompt l'installation
avec un message d'erreur explicite :

1. **Validation du manifest** : `manifest()` est appelé, le résultat est vérifié contre le schéma
   `AgentManifest` (champs obligatoires, types, format). Le fichier `manifest.json` présent dans le
   répertoire doit être identique au manifest retourné dynamiquement.

2. **Scan `dangerous_tools_allowed`** : si le manifest déclare `dangerous_tools_allowed: True`,
   un avertissement explicite est affiché et une confirmation supplémentaire est requise.
   L'installation avec `--non-interactive` est refusée pour les agents avec ce flag.

3. **Validation des packages pip** : les packages déclarés dans `packages` sont résolus via `pip
   index versions <package>` pour vérifier leur existence sur PyPI. L'installation du venv est
   différée à l'activation de l'agent (`INITIALIZING`).

4. **Test de smoke** : si un fichier `tests/test_smoke.py` existe dans le répertoire de l'agent,
   il est exécuté. Un échec bloque l'installation. Ce test est optionnel mais recommandé.

### 5. Séparation physique — `agents/bundled/` vs `agents/community/`

```
agents/
├── bundled/              ← agents distribués avec le runtime (gérés par Apollia)
│   ├── excel-worker/
│   │   ├── agent.py
│   │   └── manifest.json
│   ├── csv-data-worker/
│   ├── pdf-worker/
│   └── code-worker/
├── community/            ← agents installés par l'utilisateur
│   └── <agent-name>/
│       ├── agent.py
│       ├── manifest.json
│       └── README.md
└── tests/                ← tests partagés (conftest, fixtures)
```

Cette séparation est stricte : le runtime refuse d'installer un agent dans `bundled/` via
`apollia-os agent install`. Les bundled sont mis à jour uniquement via une mise à jour du runtime.

La migration des agents existants (actuellement à plat dans `agents/`) vers `agents/bundled/` est
effectuée dans STORY-410.

### 6. Auto-installation des bundled — au premier boot via `manifest.json` central

Un fichier `agents/bundled/registry.json` liste les quatre agents bundled avec leur chemin relatif.
Au premier démarrage du runtime (détecté via l'absence d'entrées dans la table `agents` de SQLite),
le Supervisor appelle `install_bundled_agents()` qui itère sur `registry.json` et enregistre chaque
agent dans `AgentRegistry`.

L'installation des venvs pip est déclenchée au premier `agent start <name>` (`INITIALIZING`),
pas au boot — pour ne pas bloquer le démarrage sur un hardware lent.

---

## Alternatives considérées

| Option | Raison du rejet |
|---|---|
| Tous les agents bundled dans le binaire (embed) | Binaire trop lourd, impossibilité de mise à jour indépendante des agents vs runtime |
| Endpoint distant pour les bundled (CDN/GitHub Releases) | Viole Principe #2 — le runtime ne doit pas nécessiter de réseau pour fonctionner |
| Registre centralisé hébergé par Apollia (serveur HTTP) | Infrastructure à maintenir, point de défaillance unique, principe local-first compromis |
| Pas de séparation `bundled/` vs `community/` | Confusion entre agents maintenus et agents tiers, risque de mise à jour accidentelle |
| Installation dans un dossier système global (`~/.apollia/agents/`) | Rend le projet non-portable (pas de `git clone` + lancer), complique le développement |
| Validation lazy (au premier `agent start`) | Viole Principe #4 — une erreur de manifest n'est détectée qu'à l'exécution, trop tard |
| Venv installé au boot pour les bundled | Dégrade le temps de démarrage sur hardware modeste (openpyxl : ~5 s, pandas : ~20 s) |

---

## Conséquences

**Positives :**
- Le runtime fonctionne hors ligne — les agents bundled sont inclus dans le repo, sans réseau requis.
- La séparation physique rend l'origine d'un agent immédiatement lisible dans l'arborescence.
- La validation à l'installation détecte les manifests malformés et les flags dangereux avant toute exécution.
- Les agents communautaires (`sql-worker`, `git-worker`) servent de référence de template pour les builders tiers.
- V2 (registre Git) est compatible V1 : aucune migration requise, le format `manifest.json` est stable.

**Négatives / Compromis :**
- Migration manuelle des agents existants de `agents/` vers `agents/bundled/` (STORY-410).
- `apollia-os agent install` synchrone avec confirmation : légèrement plus lent pour les scripts d'automatisation (mitigé par `--yes` flag).
- Le registre V1 (répertoire local) ne permet pas la découverte d'agents tiers sans copie manuelle — c'est intentionnel pour V1.

**Neutres / À surveiller :**
- Compatibilité multiplateforme des packages pip des bundled (openpyxl, pandas, pdfplumber) — déjà validée sur macOS, à vérifier sur Linux ARM.
- Temps total de premier boot incluant la copie des bundled dans SQLite — mesurer sur hardware modeste.
- Format `registry.json` du registre communautaire V2 — à spécifier dans STORY-411 avant d'implémenter.

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : Les agents bundled sont inclus dans le repo et installés localement. Aucun téléchargement réseau requis pour le fonctionnement de base. La distribution V2 (repo Git) est opt-in, pas obligatoire. Conforme et renforcé.
- **Principe #2 — Zéro dépendance externe** : Les bundled ne nécessitent pas de réseau. L'installation communautaire via URL Git est explicitement initiée par l'utilisateur, pas par le runtime. Les dépendances pip sont des dépendances de l'agent, pas du runtime. Conforme.
- **Principe #4 — Fail fast** : La validation (manifest + dangerous_tools_allowed + packages + smoke test) est effectuée à l'installation, pas à l'exécution. Un agent malformé ne peut pas être démarré. Renforcé.
- **Principe #7 — Garde-fous non-négociables** : Le scan `dangerous_tools_allowed` à l'installation est non-contournable en mode interactif. L'utilisateur doit confirmer explicitement. Conforme.

---

## Liens

- ADR fondateur Worker Agents : [ADR-048 — Worker Agents : expertise de domaine compilée](ADR-048-worker-agents-expertise-domaine.md)
- ADR routing A2A : [ADR-049 — Routing A2A inter-agents : discovery + invocation](ADR-049-a2a-routing-inter-agents.md)
- Document d'idéation source : `docs/internal/strategy/capabilities-architecture-ideation.md` §3 (Worker Agents) et §6 (Distribution)
- Story documentation builder : STORY-406 (Sprint 31)
- Stories d'implémentation : STORY-410 (bundled packaging), STORY-411 (community registry), STORY-408 (sql-worker), STORY-409 (git-worker)
