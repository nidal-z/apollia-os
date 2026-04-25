# La mémoire

`file-assistant` lit un fichier et le résume. Mais si vous lui soumettez le même fichier demain, il ne se souvient pas qu'il l'a déjà traité. Chaque tâche repart de zéro.

C'est l'état par défaut de tout agent sans mémoire persistante — et c'est délibéré. La mémoire, dans Apollia OS, ne s'active jamais automatiquement. Votre agent décide quand mémoriser, quoi mémoriser, et quand consulter sa mémoire. Jamais le contraire.

---

## Deux niveaux, une philosophie

La mémoire d'Apollia OS est conçue en deux niveaux d'activation :

**Niveau 1 — disponible maintenant** : trois types de mémoire persistante stockés dans SQLite local, avec recherche plein texte FTS5. Zéro dépendance externe, zéro modèle d'IA requis. C'est ce que ce chapitre couvre.

**Niveau 2 — optionnel** : recherche sémantique vectorielle avec un modèle d'embedding local (22 Mo, aucun cloud). S'active si le modèle est présent sur la machine — l'agent fonctionne sans lui.

La règle absolue : **Apollia OS ne télécharge jamais automatiquement un modèle ni n'envoie vos données mémoire à un service externe.**

---

## Les quatre types de mémoire

| Type | Stockage | Ce qu'il contient |
|---|---|---|
| **Working memory** | RAM uniquement | Variables Python dans `run()` — disparaît à la fin de la tâche |
| **Mémoire épisodique** | SQLite | Événements datés — "le 14/03, ce fichier a été résumé" |
| **Mémoire sémantique** | SQLite | Faits durables clé → valeur — "budget Acme = 15 000 €" |
| **Mémoire procédurale** | SQLite | Workflows réutilisables — "pour traiter un rapport : étape 1, 2, 3..." |

---

## Ce que vous allez apprendre

- **Section 1 — Les types** : quand utiliser chaque type, avec des exemples qui étendent `file-assistant`
- **Section 2 — La recherche FTS5** : comment `ctx.memory.search` fonctionne, les opérateurs disponibles, et la dégradation vers l'embedding vectoriel
- **Section 3 — Namespaces et isolation** : namespace privé, namespaces partagés, mémoire utilisateur globale, TTL, et la CLI de gestion
