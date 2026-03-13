# Checklist e2e — Sprint 15

> **Décision** : Option B (checklist manuelle) retenue.
> `@tauri-apps/driver` cible Tauri v1 ; pour Tauri v2, `tauri-driver` existe mais
> l'intégration WebdriverIO n'est pas suffisamment stable pour garantir des tests
> reproductibles en CI. Les `data-testid` sont en place pour migrer vers Option A
> dès que l'écosystème Tauri v2 WebDriver mûrit.

---

## Prérequis

- [ ] Application compilée : `cargo tauri build` ou `cargo tauri dev` lancé
- [ ] `agents/hello_agent.py` disponible dans le répertoire projet
- [ ] Runtime Apollia OS fonctionnel (port 7771 accessible)
- [ ] Terminal ouvert à côté pour les vérifications CLI

---

## Scénario 1 : Démarrage

**Objectif** : Vérifier que l'application s'initialise correctement avec le runtime embedded.

| # | Étape | Résultat attendu | OK ? |
|---|-------|-------------------|------|
| 1.1 | Lancer l'application (`cargo tauri dev`) | La fenêtre principale apparaît (1280×800) | [ ] |
| 1.2 | Observer l'écran de chargement | "Loading..." s'affiche brièvement (`[data-testid="app-loading"]`) | [ ] |
| 1.3 | Vérifier l'affichage principal | La vue Agents est affichée (`[data-testid="agents-header"]` visible, texte "Agents") | [ ] |
| 1.4 | Vérifier le status SSE | Point vert + texte "Runtime connected" dans le bas de la sidebar (`[data-testid="connection-status"]` avec `data-status="connected"`) | [ ] |
| 1.5 | Compter les entrées de navigation | 10 boutons de navigation dans la sidebar : Agents, Tasks, Approvals, LLM, Triggers, Pipelines, Memory, Notifications, Observability, Settings (`[data-testid^="nav-"]`) | [ ] |
| 1.6 | Vérifier le logo | "Apollia OS" affiché en haut de la sidebar (`[data-testid="sidebar-logo"]`) | [ ] |

**Critère de réussite** : Tous les points 1.1→1.6 validés.

---

## Scénario 2 : Cycle agent (register → active → stop)

**Objectif** : Vérifier le cycle de vie complet d'un agent.

| # | Étape | Résultat attendu | OK ? |
|---|-------|-------------------|------|
| 2.1 | Cliquer "Register Agent" (`[data-testid="register-agent-btn"]`) | Le file picker natif s'ouvre | [ ] |
| 2.2 | Sélectionner `agents/hello_agent.py` | Le picker se ferme, bouton passe à "Starting..." | [ ] |
| 2.3 | Observer la grille d'agents (`[data-testid="agents-grid"]`) | Un `AgentCard` apparaît (`[data-testid="agent-card"]`) | [ ] |
| 2.4 | Vérifier le statut initial | Le badge affiche "INITIALIZING" (`[data-testid="agent-status"]`) | [ ] |
| 2.5 | Attendre la transition | Le badge passe à "ACTIVE" (point vert, `data-agent-state="active"`) | [ ] |
| 2.6 | Vérifier le nom de l'agent | Le nom correspond au `manifest()` de `hello_agent.py` (`[data-testid="agent-name"]`) | [ ] |
| 2.7 | Cliquer "Stop" (`[data-testid="agent-stop-btn"]`) | Le dialogue de confirmation apparaît ("Stop this agent?") | [ ] |
| 2.8 | Cliquer "Confirm" (`[data-testid="agent-stop-confirm-btn"]`) | Le bouton passe à "Stopping..." | [ ] |
| 2.9 | Attendre la transition | Le badge passe à "STOPPED" (`data-agent-state="stopped"`) | [ ] |
| 2.10 | Vérifier CLI (terminal) | `apollia-os agent list` montre l'agent en état STOPPED | [ ] |

**Critère de réussite** : Tous les points 2.1→2.10 validés. L'agent passe par INITIALIZING → ACTIVE → STOPPED.

---

## Scénario 3 : Submit task → timeline visible

**Objectif** : Vérifier la soumission d'une tâche et l'affichage de la timeline.

**Prérequis** : Un agent ACTIVE (reprendre après scénario 2 avec un nouvel agent ou relancer hello_agent).

| # | Étape | Résultat attendu | OK ? |
|---|-------|-------------------|------|
| 3.1 | Naviguer vers Tasks (`[data-testid="nav-tasks"]`) | La vue Tasks s'affiche (`[data-testid="tasks-header"]`) | [ ] |
| 3.2 | Cliquer "New Task" (`[data-testid="new-task-btn"]`) | Le dialog "New Task" s'ouvre (`[data-testid="new-task-dialog"]`) | [ ] |
| 3.3 | Sélectionner l'agent dans le dropdown (`[data-testid="new-task-agent-select"]`) | L'agent ACTIVE apparaît dans la liste | [ ] |
| 3.4 | Saisir un input dans le textarea (`[data-testid="new-task-input"]`) | Le compteur de caractères s'incrémente | [ ] |
| 3.5 | Cliquer "Submit" (`[data-testid="new-task-submit-btn"]`) | Le bouton passe à "Submitting...", le dialog se ferme | [ ] |
| 3.6 | Observer la liste de tâches | Une nouvelle ligne apparaît (`[data-testid="task-row"]`) avec un badge "working" | [ ] |
| 3.7 | Attendre la complétion | Le badge de la tâche passe à "completed" (`data-task-status="completed"`) | [ ] |
| 3.8 | Cliquer sur la ligne de la tâche | Le panel TaskDetail s'ouvre à droite (`[data-testid="task-detail"]`) | [ ] |
| 3.9 | Vérifier la timeline | La section timeline (`[data-testid="task-timeline-section"]`) contient des événements (`[data-testid="timeline-event"]`) | [ ] |
| 3.10 | Vérifier les types d'événements | Au moins un `task_transition` et un `task_completed` visibles (`data-event-type`) | [ ] |

**Critère de réussite** : Tous les points 3.1→3.10 validés. Le cycle WORKING → COMPLETED est visible en temps réel via SSE.

---

## Scénario 4 : HITL (approval flow)

**Objectif** : Vérifier le workflow d'approbation humaine.

**Prérequis** : Un agent configuré avec `tools_requiring_approval` dans son manifest. Si `hello_agent.py` ne déclenche pas d'approval, utiliser un agent de test dédié ou simuler via CLI (`apollia-os task resume --approve <task-id>`).

| # | Étape | Résultat attendu | OK ? |
|---|-------|-------------------|------|
| 4.1 | Soumettre une tâche qui déclenche `input_required` | La tâche apparaît dans la liste avec badge "input_required" | [ ] |
| 4.2 | Vérifier le badge sidebar | Un badge rouge apparaît sur "Approvals" dans la sidebar (`[data-testid="approvals-badge"]`) | [ ] |
| 4.3 | Naviguer vers Approvals (`[data-testid="nav-approvals"]`) | La vue Approvals s'affiche (`[data-testid="approvals-header"]`) | [ ] |
| 4.4 | Vérifier le compteur pending | Le badge affiche "N pending" (`[data-testid="approvals-pending-count"]`) | [ ] |
| 4.5 | Vérifier l'ApprovalCard | Un card s'affiche (`[data-testid="approval-card"]`) avec le prompt de l'agent | [ ] |
| 4.6 | Vérifier le timer | Le badge elapsed affiche un compteur (ex: "12s", "1m 30s") | [ ] |
| 4.7 | Cliquer "Approve" (`[data-testid="approval-approve-btn"]`) | Le dialogue de confirmation apparaît | [ ] |
| 4.8 | Cliquer "Confirm" (`[data-testid="approval-confirm-btn"]`) | Le card disparaît (resolved = true) | [ ] |
| 4.9 | Vérifier la section History | L'approbation apparaît dans "History (last 7 days)" | [ ] |
| 4.10 | Vérifier la tâche | Retourner dans Tasks : la tâche passe de "input_required" → "working" → "completed" | [ ] |

**Critère de réussite** : Tous les points 4.1→4.10 validés. Le flux HITL complet est fonctionnel.

**Note** : Si aucun agent de test ne déclenche d'approval, valider les étapes 4.3→4.6 avec la vue vide ("Aucune approbation en attente") et noter dans les résultats.

---

## Scénario 5 : System tray (close → hide → reopen → quit)

**Objectif** : Vérifier le comportement du system tray et la persistance du runtime.

| # | Étape | Résultat attendu | OK ? |
|---|-------|-------------------|------|
| 5.1 | Vérifier la présence de l'icône tray | L'icône Apollia OS est visible dans la barre des tâches / menu bar | [ ] |
| 5.2 | Fermer la fenêtre (bouton ×) | La fenêtre disparaît mais l'icône tray reste | [ ] |
| 5.3 | Vérifier le runtime (terminal) | `apollia-os status` confirme que le runtime tourne toujours | [ ] |
| 5.4 | Cliquer l'icône tray | La fenêtre réapparaît au premier plan | [ ] |
| 5.5 | Vérifier l'état SSE | Le status indicator est toujours "Runtime connected" (pas de reconnexion) | [ ] |
| 5.6 | Vérifier les agents | Les agents restent dans leur état précédent (pas de reset) | [ ] |
| 5.7 | Faire clic droit sur l'icône tray | Le menu contextuel apparaît ("Ouvrir Apollia OS", "Quitter") | [ ] |
| 5.8 | Cliquer "Quitter" dans le menu tray | La fenêtre se ferme, l'icône tray disparaît | [ ] |
| 5.9 | Vérifier l'arrêt du runtime (terminal) | `apollia-os status` retourne une erreur (connexion refusée) | [ ] |
| 5.10 | Vérifier le process | Le process `apollia-desktop` n'est plus dans `ps aux` | [ ] |

**Critère de réussite** : Tous les points 5.1→5.10 validés. Fermer la fenêtre masque l'app, "Quitter" arrête proprement.

---

## Résultats d'exécution

| Scénario | Date | Résultat | Notes |
|----------|------|----------|-------|
| 1 — Démarrage | | | |
| 2 — Cycle agent | | | |
| 3 — Submit task + timeline | | | |
| 4 — HITL | | | |
| 5 — System tray | | | |

**Testeur** :
**Version** :
**OS** :
