---
title: Approuver ou refuser une action d'agent
slug: /operator-help/control/approve-or-reject-an-action
sidebar_position: 1
---

# Approuver ou refuser une action d'agent

> Pour les operators qui veulent garder la main sur chaque action sensible déclenchée par un agent (écriture, commande, appel d'outil externe).

> **Note - paliers d'autonomie :** le flux d'approbation dépend du palier choisi au lancement. En palier `assisted` (défaut), toute action sensible passe par votre validation, comme décrit sur cette page. A partir du palier `supervised`, la boucle de vérification automatique peut corriger les anomalies sans vous solliciter et ne soumet à votre approbation que ce qui résiste à cette correction. Les paliers `bounded_autonomous` et `long_autonomous` réduisent encore davantage les interruptions. Voir [Paliers d'autonomie](../agents/choose-an-autonomy-level.md).

## Prérequis

- Un agent en cours d'exécution sur une tâche qui touche des fichiers, des commandes ou des outils externes.
- Vous comprenez ce que l'agent est censé faire (la mission est claire pour vous).

## Où la demande d'accord apparaît

Une demande d'accord s'affiche à deux endroits :

- **Dans le chat** : une carte s'insère dans la conversation, à l'endroit où l'agent a demandé. Elle dit ce que l'agent veut faire et porte les réponses.

- **Dans la barre latérale, sous À valider** : une seule liste réunit tout ce qui vous attend, quelle que soit la conversation, la tâche ou le lot d'où cela vient. Une demande posée dans une conversation figure aux deux endroits, et y répondre à l'un la retire de l'autre. Voir [Utiliser la liste À valider](../transversal/use-the-inbox.md).

> **Note :** les demandes apparaissent sans rafraîchir. Le badge à côté de **À valider**, dans la barre latérale, compte ce qui attend.

## Les trois réponses

Une demande d'accord a trois réponses, et rien d'autre. Il n'y a pas de portée à choisir.

1. **Une fois** : l'action s'exécute, pour cette demande seulement. La prochaine action du même type redemande.

2. **Toujours pour cet agent** : l'action s'exécute, et cet agent peut refaire ce type d'action sans vous redemander. L'accord vaut pour l'agent qui a demandé et pour la famille de l'action (par exemple écrire ou supprimer), pas pour les autres agents. Vous le retirez à tout moment dans l'écran **Autorisations** (voir [Gérer les autorisations d'outils](manage-tool-permissions.md)).

3. **Refuser** : l'action ne s'exécute pas. Le champ à côté du bouton reçoit une raison, facultative : laissez-le vide et cliquez sur **Refuser**, ou tapez d'abord une courte phrase (500 caractères au plus). La raison est transmise à l'agent, ce qui lui permet de corriger sa trajectoire au lieu de retenter la même chose.

Une action de la famille **Lire** ne demande aucun accord : l'agent n'est pas interrompu. Les autres familles (**Lire des données personnelles**, **Écrire ou supprimer**, **Envoyer vers l'extérieur**, **Exécuter du code**) demandent.

<!-- claim:chat-tool-governance-path -->
> **Cas particulier du code** (les outils Bash et Python) : **Toujours pour cet agent** n'est jamais proposé pour du code. Une commande ou un script peut faire tout ce que peut l'interpréteur, et un accord donné à toute la famille serait un chèque en blanc. Par défaut, chaque exécution demande votre accord, avec **Une fois** et **Refuser** pour seules réponses.
<!-- claim:prefix-rules-evaluated-per-invocation -->
> Mode développeur allumé, une commande simple propose aussi **Toujours autoriser cette commande** : cet agent peut alors relancer exactement cette commande sans vous redemander, et vous le retirez dans l'écran **Autorisations**. La règle est vérifiée à chaque exécution. Elle n'est pas proposée sur une commande qui en enchaîne plusieurs (avec `;`, `&&`, un pipe, une redirection ou une substitution) : chaque exécution d'une telle commande demande votre accord.

<!-- claim:bash-executor-requires-posix-shell -->
> **Sous Windows**, ces deux outils ont aussi des prérequis sur l'hôte : un shell POSIX dans le `PATH` pour Bash (Git Bash, MSYS2 ou WSL) et un Python 3 installé pour Python. Quand l'un manque, l'appel échoue avec un message qui nomme le prérequis au lieu de s'exécuter.

> **Cas particuliers à deux réponses :** une tâche qui s'est mise en pause pour vous attendre, et un fait qu'un agent propose pour votre profil, n'offrent que **Une fois** et **Refuser**. Ni l'un ni l'autre ne s'accorde pour de bon.

## Approuver un plan (mode plan)

Quand l'assistant travaille en **mode plan**, une carte distincte apparaît dans le chat une fois le plan prêt : la carte de revue du plan, qui liste les étapes proposées avec les actions **Approuver** et **Demander des ajustements**. Ce contrôle est distinct des demandes d'accord ci-dessus : il valide le plan entier avant le début de toute exécution.

<!-- claim:plan-approval-executes -->
Une fois que vous approuvez, l'assistant **exécute le plan approuvé** et ne le repropose pas ; si vous approuvez alors que l'assistant termine encore son tour en cours, l'exécution démarre juste après la fin de ce tour. Demander des ajustements avec un commentaire renvoie votre retour à l'assistant, qui révise le plan et le soumet à nouveau via la même carte.

## Étapes - répondre à une demande

1. Lisez ce que l'agent demande. Pour du code, lisez la commande entière : dans **À valider**, une longue commande est repliée, et les réponses apparaissent une fois que vous avez cliqué sur **Afficher la commande entière**.

2. Cliquez sur **Une fois** pour laisser passer cette seule action, ou sur **Toujours pour cet agent** pour ne plus être sollicité pour ce type d'action par cet agent.

3. Pour rejeter, tapez une raison utile à l'agent (par exemple *« Mauvais dossier, utilise ./tmp à la place »* plutôt que *« Non »*), puis cliquez sur **Refuser**.

4. L'agent reçoit aussitôt votre réponse et poursuit : avec le résultat de l'action, ou avec votre refus et sa raison.

## Relire les décisions déjà prises

La liste **À valider** ne garde aucun historique : une demande à laquelle vous avez répondu la quitte. Les décisions déjà prises se lisent dans le **Journal**, depuis la barre latérale. Chaque action des agents et chaque changement d'accès y est inscrit, avec son résultat : réussi, échoué ou refusé. Voir [Consulter la piste d'audit](../observabilite/read-the-audit-trail.md).

## Vérification

- La carte quitte le chat, et la ligne quitte **À valider**, juste après votre réponse.
- Si vous avez choisi **Toujours pour cet agent**, ouvrez **Autorisations**, sélectionnez l'agent, et vérifiez que l'accord figure sous **« Toujours » donnés**.
- Pour un refus, l'agent tient compte de la raison dans la suite de la conversation.

## Si ça ne marche pas

- **Aucune carte n'apparaît alors que l'agent semble bloqué** : ouvrez **À valider** depuis la barre latérale. Une tâche ou un lot qui tourne en arrière-plan y dépose ses demandes.
- **L'agent retente sans cesse la même action refusée** : la raison n'était peut-être pas exploitable par l'agent. Refusez de nouveau avec une raison plus actionnable (un autre chemin, la valeur attendue).
- **Un « toujours » laisse passer trop de choses** : ouvrez **Autorisations**, sélectionnez l'agent et cliquez sur **Retirer** sur l'accord. La prochaine action de cette famille redemande. Voir [Gérer les autorisations d'outils](manage-tool-permissions.md).
- **Toujours pour cet agent manque sur une demande** : c'est attendu pour du code, pour une tâche en pause et pour un fait proposé pour votre profil.
- **Moins de demandes d'accord que d'habitude** : c'est attendu si l'agent tourne en palier `supervised` ou supérieur. La boucle de vérification automatique résout une partie des situations sans vous solliciter. Si vous voulez reprendre un contrôle total, relancez l'agent avec `--autonomy assisted`.

> **Concept :** [Explication Apollia](/explanation)
