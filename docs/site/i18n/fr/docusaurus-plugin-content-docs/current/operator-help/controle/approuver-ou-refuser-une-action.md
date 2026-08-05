# Approuver ou refuser une action d'agent

> Pour les operators qui veulent garder la main sur chaque action sensible déclenchée par un agent (écriture, commande, appel d'outil externe).

> **Note - paliers d'autonomie :** le flux d'approbation dépend du palier choisi au lancement. En palier `assisted` (défaut), toute action sensible passe par votre validation, comme décrit sur cette page. A partir du palier `supervised`, la boucle de vérification automatique peut corriger les anomalies sans vous solliciter et ne soumet à votre approbation que ce qui résiste à cette correction. Les paliers `bounded_autonomous` et `long_autonomous` réduisent encore davantage les interruptions. Voir [Paliers d'autonomie](../agents/choisir-un-palier-d-autonomie.md).

## Prérequis

- Un agent en cours d'exécution sur une tâche qui touche fichiers, commandes ou outils externes.
- Vous comprenez ce que l'agent est censé faire (la mission est claire pour vous).

## Où la demande d'approbation apparaît

Une demande d'approbation peut surgir à deux endroits, selon le contexte :

- **Dans le chat** (avec Apollia Chat ou un agent conversationnel) : une **carte d'approbation** s'insère dans le flux des messages, à la position chronologique de la demande. La carte porte une icône bouclier ⛨ et une bordure orange.
  ![Carte d'approbation en ligne dans le chat, icone bouclier orange, apercu de la commande a autoriser, et les boutons Autoriser une fois, Refuser et Toujours autoriser](/img/operator-help/controle-approuver-ou-refuser-une-action-1.png)

- **Dans la sidebar → Approbations** (qui ouvre la page **Boîte de réception**) : pour les agents qui tournent en arrière-plan ou qui ont mis leur tâche en pause pour vérification humaine. Chaque demande apparaît sous forme d'une ligne dans une liste groupée par date (Aujourd'hui / Hier / Plus tôt). Cliquer une ligne déplie la **carte HITL** avec les détails et les boutons d'action.
  ![Page Boite de reception, puces de filtre en haut et carte d'approbation depliee affichant son badge de risque](/img/operator-help/controle-approuver-ou-refuser-une-action-2.png)

> **Note :** Les demandes apparaissent **en temps réel** sans rafraîchissement. Un compteur dans le sous-titre de la page indique le nombre total en attente.

## Les trois décisions possibles

Quel que soit le point d'entrée (chat ou Boîte de réception pour un appel d'outil), les actions disponibles sont les mêmes :

1. **Autoriser une fois** - l'action s'exécute immédiatement pour cette demande uniquement. L'agent reprend, la prochaine occurrence redemandera confirmation.

2. **Refuser** - un dialog **Raison du refus** s'ouvre. Saisissez une explication de **5 à 500 caractères** (compteur en bas du textarea) puis confirmez. Le bouton n'est actif qu'à partir de 5 caractères.
   ![dialog Raison du refus avec textarea, compteur "12 / 500", boutons Annuler / Confirmer le refus en bas](/img/operator-help/controle-approuver-ou-refuser-une-action-3.png)

   La raison est **transmise à l'agent** : elle est injectée dans le message d'outil que voit le LLM à l'itération suivante, sous la forme *« Outil refusé par l'utilisateur. Raison : … »*. Cela permet à l'agent de corriger sa trajectoire plutôt que de retenter aveuglément. La raison est aussi **persistée** dans l'historique récent (voir plus bas) pour retrouver le contexte plus tard.

3. **Toujours autoriser** - ouvre un menu avec **4 portées** au choix, dans la carte du chat :

   | Portée | Effet |
   |---|---|
   | **Pour cette session** | Auto-approuvé jusqu'à la fermeture du chat. Non persistée. |
   | **Toujours pour cet assistant** | Règle persistée - l'assistant courant ne redemandera plus pour cet outil. |
   | **Toujours pour ce projet** | Règle persistée pour tous les assistants utilisés dans le projet courant. *Désactivée si la session n'est rattachée à aucun projet.* |
   | **Toujours, partout** | Règle persistée globalement - tous les assistants, tous les projets. Affichée en orange comme signal de la portée maximale. |

   La demande d'approbation déclenchée par une écriture fichier jugée risquée est une autre surface, et elle en propose **cinq** : les quatre ci-dessus plus **Cet outil uniquement**, qui autorise ce seul outil et rien d'autre.

   Les règles persistées sont consultables et révocables dans **Paramètres → Autorisations** (voir [Gérer les autorisations d'outils](configurer-les-permissions-de-fichiers.md)).

<!-- claim:chat-tool-governance-path -->
> **Cas particulier des exécuteurs de code** (`bash_executor`, `python_executor`) : *Toujours autoriser* n'est jamais honoré pour eux, quelle que soit la portée choisie. Leur argument est une commande shell ou du code arbitraire ; une autorisation en bloc serait un blanc-seing sur tout l'interpréteur. L'appel courant est bien exécuté une fois, mais l'invocation suivante redemande une confirmation.
<!-- claim:prefix-rules-evaluated-per-invocation -->
> Pour auto-approuver une commande précise, configurez une règle de préfixe ciblée dans **Paramètres → Autorisations** : elle est évaluée à chaque invocation et ne s'applique qu'à une commande simple unique (sans enchaînement `;`, `&&`, pipe, redirection ni substitution). Tout ce qui dépasse le préfixe, ou toute commande enchaînée, redemande une confirmation comme avant.

<!-- claim:bash-executor-requires-posix-shell -->
> **Sous Windows**, ces deux outils exigent aussi des prérequis sur la machine : un shell POSIX dans le `PATH` pour `bash_executor` (Git Bash, MSYS2 ou WSL) et un Python 3 installé pour `python_executor`. S'il en manque un, l'appel échoue avec un message nommant le prérequis au lieu de s'exécuter.

> **Cas particulier des tâches en pause** (« approbation de tâche ») : un agent qui se suspend lui-même via un point de contrôle HITL n'expose qu'**Autoriser** / **Refuser** (pas de *Toujours autoriser*) puisqu'il ne s'agit pas d'un outil mémorisable. Le dialog de raison reste obligatoire au refus.

## Approuver un plan (mode plan)

Quand l'assistant travaille en **mode plan**, une carte distincte apparaît dans le chat dès que le plan est prêt : la carte de relecture du plan, qui liste les étapes proposées avec les actions **Approuver** et **Demander des changements**. Cette validation est séparée des approbations d'outils ci-dessus : elle valide le plan entier avant tout début d'exécution.

<!-- claim:plan-approval-executes -->
Une fois le plan approuvé, l'assistant **exécute le plan approuvé** et ne le repropose pas ; si vous approuvez pendant que l'assistant termine encore son tour en cours, l'exécution démarre juste après la fin de ce tour. Un refus accompagné d'un commentaire transmet votre retour à l'assistant, qui révise le plan et le soumet à nouveau via la même carte.

## Étapes - résolution d'une demande

1. Cliquez sur **Autoriser une fois** pour valider ponctuellement, ou ouvrez le menu **Toujours autoriser** pour installer une règle persistante.

2. Pour refuser, cliquez sur **Refuser** : le dialog de raison s'ouvre. Tapez une explication courte mais utile à l'agent (ex. *« Mauvais dossier - utilise ./tmp à la place »* plutôt que *« Non »*), puis cliquez sur **Confirmer le refus**.

3. La carte disparaît du chat (ou de la Boîte de réception), un toast confirme la décision (*« Action approuvée »* / *« Action refusée »* / *« Règle enregistrée - futurs appels auto-approuvés »*).

4. Dans le chat, l'agent reçoit immédiatement le résultat (refus + raison, ou résultat de l'outil) et poursuit sa réflexion à la prochaine itération de raisonnement.

## Consulter l'historique des décisions

Au bas de la page **Boîte de réception**, sous la liste des actions en attente, une section **Historique récent (14 jours)** affiche les **50 dernières** décisions HITL résolues (chronologie inverse) :

- Icône colorée : ✅ Autorisé (vert) · 🛡 Toujours autorisé (bleu primaire) · ❌ Refusé (rouge).
- Nom de l'outil concerné.
- Pour les refus : la **raison saisie** au moment du refus, en rouge.
- Horodatage relatif (`5min ago`, `2h ago`…) avec date absolue en tooltip.
- Préfixe court de la session d'origine.

![section Historique récent - quatre lignes avec icônes différentes, un refus avec sa raison affichée en rouge](/img/operator-help/controle-approuver-ou-refuser-une-action-4.png)

L'historique est en **lecture seule** ; il ne se remplace pas par la page Paramètres → Autorisations → Audit récent, qui affiche en plus les décisions automatiques (déclenchées par règles persistées) sur 20 entrées.

## Vérification

- La carte d'approbation disparaît du chat (ou la ligne de la Boîte de réception) immédiatement après votre décision.
- Un toast confirme l'opération.
- Si vous avez choisi **Toujours autoriser**, ouvrez **Paramètres → Autorisations** et vérifiez qu'une nouvelle règle apparaît dans la liste, avec le bon scope.
- Pour un refus, l'agent doit prendre en compte la raison à sa prochaine itération (vous le verrez dans la suite de la conversation ou dans les logs de l'agent).

## Si ça ne marche pas

- **Aucune carte n'apparaît alors que l'agent semble bloqué** : ouvrez la **Boîte de réception** depuis la sidebar. Les agents en arrière-plan y déposent leurs demandes au lieu de les afficher dans le chat.
- **L'agent réessaie sans cesse la même action refusée** : la raison n'était peut-être pas exploitable par l'agent. Ouvrez ses logs depuis **Mes assistants** ; la raison transmise s'y retrouve dans la sortie de l'outil refusé. Refusez à nouveau avec une raison plus actionnable (chemin alternatif, valeur attendue…).
- **Une règle "Toujours" crée trop d'actions automatiques** : ouvrez **Paramètres → Autorisations** et révoquez ou affinez le périmètre de la règle. Voir [Gérer les autorisations d'outils](configurer-les-permissions-de-fichiers.md).
- **L'option "Toujours pour ce projet" est grisée** : la session de chat courante n'est rattachée à aucun projet. Liez-la depuis l'en-tête du chat, ou utilisez la portée *Toujours pour cet assistant* à la place.
- **Moins de demandes d'approbation que d'habitude** : c'est normal si l'agent tourne en palier `supervised` ou supérieur. La boucle de vérification automatique résout une partie des situations sans vous solliciter. Si vous souhaitez rétablir un contrôle complet, relancez l'agent avec `--autonomy assisted`.

> **Concept :** [Explication Apollia](/explanation)
