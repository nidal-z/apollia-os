# Phase A — Plan d'audit exhaustif des 3 sources documentaires

> Document de planification opérationnelle.
> Pré-requis : `CONTEXTE-DEPART.md` validé (charte L1, tableau propriété, arborescence cible help/).
> Sortie : 5 livrables d'audit qui pilotent Phase B (mise en conformité), avant Phase C (skill auto-sync).
> Date : 2026-04-24. Sprint référence : 40.

---

## 1. Objectif mesurable

À la fin de Phase A, on doit pouvoir répondre **oui/non** à la question :

> *"Pour chaque feature livrée jusqu'au sprint 40, est-elle couverte par exactement le bon système (book et/ou wiki et/ou help) selon la règle de propriété de la charte L1.2 — et le contenu correspond-il au code actuel ?"*

Si non : on sait précisément quoi **créer**, **modifier**, **scinder**, ou **supprimer**, avec une estimation d'effort par item.

Phase A ne modifie ni le book, ni le wiki, ni ne crée le help. Elle **audite et planifie** uniquement.

---

## 2. Pré-requis avant lancement

À vérifier en 5 minutes avant de lancer le premier agent :

- [ ] `CONTEXTE-DEPART.md` est à jour à la racine du repo.
- [ ] `docs/wiki/Sprint-Summary.md` reflète bien le sprint 40 livré.
- [ ] Aucun PR documentation en cours (`gh pr list --label docs`) — sinon on audite sur du sable mouvant.
- [ ] Le workspace compile (`cargo check --workspace`) — sinon l'inventaire code Axe 1 contiendra des leurres.
- [ ] Le répertoire `docs/internal/audit/` existe ou est à créer (dossier de sortie des 5 livrables).

---

## 3. Méthode d'audit — 4 axes parallélisables + 1 synthèse

L'audit est découpé en **5 sous-tâches** strictement séquencées dans l'ordre suivant : Axe 1 seul → Axes 2/3/4 en parallèle → Axe 5 (synthèse main thread).

### Axe 1 — Inventaire exhaustif du code livré

**Sortie** : `docs/internal/audit/01-inventaire-code-livre.md`

**Pourquoi en premier** : ce document est la **source de vérité** consommée par les axes 2, 3, 4. Sa qualité conditionne tout le reste. Il ne sera relancé qu'à chaque sprint majeur.

**Méthode** :

1. Pour **chaque crate** du workspace `crates/apollia-*` :
   - Lister les modules publics (`pub mod`).
   - Lister les types publics significatifs (`pub struct`, `pub enum`, `pub trait`).
   - Lister les fonctions/méthodes exposées au SDK Python (via `apollia-aip` ou `#[pyo3]`).
   - Lister les structs de configuration (lus depuis `apollia.toml` ou variables d'env).
   - Lister les variantes d'erreur (`thiserror` enum dans `error.rs`).
2. Pour `crates/apollia-desktop/ui/src/` :
   - Lister chaque **route Svelte** (`routes/*.svelte`) avec son chemin URL et son rôle fonctionnel.
   - Lister chaque **dialog / modal** majeur (`components/**/Create*.svelte`, `Edit*.svelte`, `*Dialog.svelte`).
   - Lister chaque **commande IPC Tauri** exposée (`crates/apollia-desktop/src/commands/**/*.rs`, attribut `#[tauri::command]`).
3. Pour chaque feature listée, croiser avec `docs/internal/STORIES/sprint-N/story-NNN.md` pour identifier **dans quel sprint elle a été livrée** (utile pour Phase C : seuls les commits qui touchent une crate listée déclenchent la mise à jour doc).

**Format de sortie** : 3 grands tableaux markdown (Crates Rust | Surface UI | Commandes IPC), chaque ligne = un élément public, colonnes : `Identifiant | Crate / Route / Commande | Type | Sprint d'introduction | Statut (livré / déprécié)`.

**Critère de qualité** : zéro feature livrée non listée. Test : prendre 10 stories au hasard, vérifier que tous leurs livrables apparaissent dans l'inventaire.

**Effort estimé** : 1 agent Explore "very thorough", ~25 min wall time.

---

### Axe 2 — Audit couverture book

**Sortie** : `docs/internal/audit/02-audit-couverture-book.md`

**Méthode** :

1. Lire `book/src/SUMMARY.md` + un balayage rapide de chaque chapitre `ch01-ch19` + annexes A-F pour cataloguer **ce qui est effectivement traité** (pas juste les titres — vérifier le contenu).
2. Pour **chaque concept builder** de l'inventaire Axe 1 (types AIP, services RuntimeContext, outils natifs, patterns d'agent, ORIA, A2A, pipelines DSL Python, adapters), marquer dans une matrice :
   - `✅ Couvert` — chapitre X traite ce concept narrativement.
   - `⚠️ Partiel` — mention rapide, exemples manquants, ou patterns essentiels absents.
   - `❌ Absent` — feature livrée non couverte.
3. **Détection d'obsolescence** : pour chaque exemple de code dans le book, vérifier qu'il référence des API qui existent encore (signature, nom de méthode, import). Lister les exemples cassés.
4. **Audit charte L1.4** appliqué au book :
   - Règle 2 : chaque chapitre fait écrire ou modifier du code ? Lister les chapitres qui ne le font pas.
   - Règle 8 : aucune table de référence > 10 lignes dans le book ? Lister les tables détectées.
   - Règle 10 : sous-sections > 800 mots ? Lister les outliers.

**Format de sortie** : tableau principal `Concept | Sprint d'introduction | Status book | Chapitre couvrant | Action recommandée (créer chap / MAJ section / supprimer / RAS) | Effort (S/M/L)`. Plus 3 sous-tableaux : exemples obsolètes, chapitres sans code, tables interdites détectées.

**Critère de qualité** : la matrice contient une ligne par concept de l'inventaire Axe 1, statut renseigné systématiquement.

**Effort estimé** : 1 agent Explore "very thorough", ~30 min wall time.

---

### Axe 3 — Audit couverture wiki

**Sortie** : `docs/internal/audit/03-audit-couverture-wiki.md`

**Méthode** :

1. Lister exhaustivement `docs/wiki/*.md` (~69 pages) + `docs/Briques-*.md` + `docs/Architecture-*.md`.
2. Pour **chaque brique du workspace** (1 crate Rust = 1 page `Briques-X.md` attendue), vérifier :
   - Existence de la page.
   - Présence des sections canoniques : *Vue d'ensemble*, *Configuration*, *API publique* (signatures), *Codes d'erreur*, *Exemples d'usage*.
   - Cross-check des signatures avec le code réel (l'API documentée existe encore ?).
   - Cross-check des paramètres de config avec la struct Rust correspondante.
3. **Détection des pages narratives** (à transformer en stub ou supprimer) — au-delà des 6 déjà identifiées en L1.3 du `CONTEXTE-DEPART.md`. Critère : page contenant "Tutoriel", "Premiers pas", "Comment démarrer", "Étape par étape" dans un titre H1/H2.
4. **Détection des pages orphelines** : pages `docs/wiki/*.md` qui ne sont liées par aucune autre page (perdues).
5. **Audit charte L1.4** appliqué au wiki :
   - Règle 3 : pages = référence consultée, pas lue de bout en bout. Lister les pages > 1500 lignes (probablement à scinder).
   - Règle 7 : aucune capture d'écran dans le wiki. Vérifier (rare mais possible).
   - Règle 10 : pages > 800 mots hors tables ? Lister.

**Format de sortie** : tableau principal `Page wiki | Brique correspondante | Statut (à jour / obsolète / narrative à stubber / orpheline / RAS) | Action | Effort`. Plus 3 sous-tableaux : signatures obsolètes détectées, pages > 1500 lignes, pages orphelines.

**Critère de qualité** : chaque crate du workspace a son entrée auditée. Aucune page wiki non listée.

**Effort estimé** : 1 agent Explore "very thorough", ~30 min wall time.

---

### Axe 4 — Inventaire workflows operator pour le help

**Sortie** : `docs/internal/audit/04-inventaire-help-operator.md`

**Pourquoi cet axe est différent** : le help **n'existe pas encore**. On n'audite pas l'existant, on **catalogue les pages à créer** à partir de l'UI réelle.

**Méthode** :

1. Pour chaque route Svelte de `crates/apollia-desktop/ui/src/routes/*.svelte` :
   - Identifier le ou les workflows operator que la page expose.
   - Lister les actions principales (boutons primaires, dialogs déclenchés).
2. Pour chaque dialog/modal majeur (`Create*.svelte`, `Edit*.svelte`, `*Dialog.svelte`, `*Wizard.svelte`) :
   - Identifier le workflow déclenché.
   - Lister les champs requis et le résultat attendu.
3. **Cross-check** avec l'arborescence cible `help/` définie en L1.5 du `CONTEXTE-DEPART.md` :
   - Routes/dialogs **couverts** par une page prévue : OK.
   - Routes/dialogs **non couverts** : pages à ajouter à l'arborescence.
   - Pages prévues **sans route source** identifiée : à challenger (page utile ? à fusionner ?).
4. Pour chaque page help à créer, produire un **squelette** :
   - Titre verbe+objet (style L4 du `CONTEXTE-DEPART.md`).
   - Route source (chemin du fichier Svelte).
   - Composants UI principaux référencés.
   - Prérequis pressentis.
   - 5 à 10 étapes en bullets (sans rédaction complète).
5. Identifier les **liens sortants attendus** : pour chaque page, est-ce qu'elle pointera vers le book (concept) ou vers le wiki (spec) ? Une seule des deux.

**Format de sortie** : tableau `Page help à créer | Route source | Composants UI | Lien sortant prévu (book ch / wiki page) | Priorité (top 5 / top 15 / reste) | Effort`. Plus le squelette détaillé pour chacune des ~30 pages cible.

**Critère de qualité** : chaque route Svelte est référencée au moins une fois. Aucun dialog majeur orphelin. Arborescence finale ≤ 35 pages (sinon on sur-segmente).

**Effort estimé** : 1 agent Explore "very thorough", ~35 min wall time.

---

### Axe 5 — Synthèse cross-référencée (main thread)

**Sortie** : `docs/internal/audit/05-matrice-couverture-globale.md` + `docs/internal/audit/06-plan-phase-b.md`

**À exécuter UNIQUEMENT après axes 2/3/4 complets**, par le thread principal (pas un agent — c'est de l'arbitrage qui exige le contexte du `CONTEXTE-DEPART.md`).

**Méthode** :

1. Construire la **matrice de couverture globale** : une ligne par feature de l'inventaire Axe 1, colonnes `Book | Wiki | Help`, valeurs `✅ / ⚠️ / ❌ / N/A` (N/A = la charte L1.2 n'attend pas ce système pour cette feature).
2. Détecter les **violations de charte** :
   - **Double couverture non autorisée** : une feature avec ✅ dans plus de colonnes que la charte L1.2 ne le permet.
   - **Trou absolu** : une feature avec ❌ partout alors que la charte attend au moins une couverture.
3. Compiler le **plan Phase B chiffré** :
   - Pages help à créer : N items, effort total estimé.
   - Pages wiki à supprimer : N items.
   - Pages wiki à réécrire en référence pure : N items.
   - Chapitres book à patcher : N items.
   - Tests CI charte à ajouter : N règles automatisables.
4. Proposer un **séquencement** Phase B en sous-sprints (par exemple : sous-sprint B1 = cleanup wiki, B2 = création help vague 1, B3 = patches book, B4 = tests CI).
5. Documenter les **arbitrages ouverts** que Phase A ne peut pas trancher seule (ex : "page X a 2 implémentations possibles, Nidal doit choisir").

**Format de sortie** :
- `05-matrice-couverture-globale.md` : un grand tableau filtrable (markdown table avec ~80-120 lignes) + 2 sous-tableaux Violations.
- `06-plan-phase-b.md` : checklist actionnable avec items priorisés, effort, dépendances entre items, et arbitrages à valider par Nidal.

**Critère de qualité** : la matrice ne contient aucune cellule vide. Le plan Phase B contient un chiffrage effort cohérent (jours-homme).

**Effort estimé** : main thread, ~45 min de réflexion + rédaction.

---

## 4. Ordre d'exécution strict

```
[T0]  Pré-requis vérifiés (5 min)
        │
        ▼
[T0+5]  Axe 1 (inventaire code) — 1 agent, ~25 min
        │
        ▼
[T0+30] Validation manuelle Axe 1 (5 min) — checkpoint critique
        │
        ▼
[T0+35] Lancement parallèle Axes 2 + 3 + 4 — 3 agents simultanés, ~35 min
        │
        ▼
[T0+70] Axe 5 (synthèse main thread) — ~45 min
        │
        ▼
[T0+115] Phase A terminée : 5 livrables produits
```

**Wall time total** : ~2 heures.
**Charge cognitive Nidal** : 1 checkpoint intermédiaire (validation Axe 1) + relecture finale des 2 livrables de synthèse (~30 min).

---

## 5. Critères d'acceptation Phase A

- [ ] 6 fichiers présents dans `docs/internal/audit/` (numérotés 01 à 06).
- [ ] Matrice de couverture globale sans cellule vide.
- [ ] Aucune feature de l'inventaire Axe 1 absente de la matrice de l'Axe 5.
- [ ] Plan Phase B contient une estimation effort (S/M/L ou jours) par item.
- [ ] Charte L1.4 testée mécaniquement sur les 3 systèmes (résultat tabulaire dans chaque livrable d'axe).
- [ ] Liste explicite des arbitrages ouverts à trancher par Nidal (≤ 10 items).
- [ ] Aucune modification du book, du wiki, ou création du help (Phase A est read-only sauf sur `docs/internal/audit/`).

---

## 6. Risques identifiés et mitigation

| # | Risque | Impact | Mitigation |
|---|---|---|---|
| 1 | Inventaire Axe 1 incomplet → cascade sur axes 2/3/4 | Élevé | Checkpoint manuel obligatoire après Axe 1, avant lancement parallèle. |
| 2 | Pages wiki narratives plus nombreuses que prévu (>20) → cleanup sous-estimé | Moyen | Dans Axe 3, lister chaque page narrative individuellement, pas en batch. |
| 3 | Workflows operator complexes nécessitant test UI réel | Moyen | Axe 4 cite la route Svelte source pour chaque page. Validation visuelle déléguée à Phase B. |
| 4 | Dérive de scope vers exécution Phase B | Élevé | Critère d'acceptation explicite : zéro modif hors `docs/internal/audit/`. |
| 5 | Sprint 41 commence pendant Phase A → inventaire désynchronisé | Faible | Geler tout commit feature pendant les ~2h de Phase A, ou rejouer Axe 1 après. |
| 6 | Charte L1.4 ambiguë sur certaines règles (ex : règle 2 sur "chapitre = code") | Moyen | Phase A produit la liste des cas litigieux, Nidal arbitre dans l'Axe 5. |

---

## 7. Hors scope (réservé Phase B)

- Création des pages help.
- Cleanup des redondances wiki.
- Patches des chapitres book obsolètes.
- Mise en place des tests CI sur la charte (linter `markdownlint` custom).
- Captures d'écran réelles pour le help.
- Toute modification de `book/`, `docs/wiki/`, `docs/Briques-*.md`, `docs/Architecture-*.md`.

---

## 8. Hors scope (réservé Phase C)

- Conception du skill `doc-sync-on-commit`.
- Table de routage `code → doc` (sera dérivée de l'inventaire Axe 1, mais formalisée en Phase C).
- Hooks git, CI GitHub Action.
- Prompts Claude pour la mise à jour automatique.

---

## 9. Fichiers critiques de référence (lecture seule pendant Phase A)

| Besoin | Fichier |
|---|---|
| Charte de séparation 3 systèmes | `CONTEXTE-DEPART.md` (livrable 1) |
| Tableau de propriété thématiques | `CONTEXTE-DEPART.md` § 1.2 |
| Arborescence cible help | `CONTEXTE-DEPART.md` § 1.5 |
| Inventaire features par sprint | `docs/internal/STORIES/sprint-index.md` |
| Index book actuel | `book/src/SUMMARY.md` |
| Index wiki actuel | `docs/wiki/Home.md` |
| Surface code workspace | `Cargo.toml` (workspace members) + `crates/*/Cargo.toml` |
| Surface UI operator | `crates/apollia-desktop/ui/src/routes/` |
| Surface IPC | `crates/apollia-desktop/src/commands/` |
| Décisions architecturales | `docs/Decisions-Log.md` |

---

## 10. Format des prompts d'agents (à dérouler au lancement)

Pour traçabilité, chaque agent recevra exactement les éléments suivants en contexte :

1. Lien vers `CONTEXTE-DEPART.md` (charte L1 + tableau propriété).
2. Lien vers `PHASE-A-AUDIT-DOC.md` (ce fichier — section méthode de l'axe concerné).
3. Pour Axes 2/3/4 : lien vers la sortie de l'Axe 1 (`01-inventaire-code-livre.md`).
4. Format de sortie attendu (tableau + sous-tableaux).
5. Critères de qualité applicables.
6. Path absolu du fichier de sortie à produire.

Les prompts complets sont à rédiger au moment du lancement (pas dans ce plan, pour rester actionnable).

---

## 11. Décision de lancement

Lancement Phase A conditionné à :
- Validation par Nidal de ce plan (relecture du présent document).
- Choix d'une fenêtre de ~2h sans push feature simultané.
- Confirmation que `docs/internal/audit/` peut être créé.

**Go/No-Go signal attendu** : message explicite "Lance Phase A" de Nidal après relecture.
