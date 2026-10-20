---
title: Confier une tâche à un agent
slug: /operator-help/agents/start-an-agent
sidebar_position: 2
---

# Confier une tâche à un agent

> Pour toute personne qui a un agent installé et veut le faire travailler : lui parler, lui confier une série d'éléments, ou laisser une automatisation l'appeler.

## Prérequis

- L'agent figure dans l'écran **Agents**.
- Un modèle répond. Les modèles se règlent dans l'écran **IA**.

## Un agent ne se démarre ni ne s'arrête

Il n'y a ni bouton de démarrage ni bouton d'arrêt. Un agent est chargé par sa première tâche et libéré une fois inactif. Ce que montre l'écran **Agents**, c'est si une tâche peut atteindre l'agent :

- **disponible** : l'agent prend une tâche.
- **dégradé** : l'agent prend encore une tâche, avec un avertissement.
- **indisponible** : l'agent a été désactivé depuis la ligne de commande. `apollia-os agent start <id-agent>` le rend de nouveau disponible.

L'agent intégré **Apollia Chat**, en tête de la colonne **Agents**, est toujours disponible.

## Étapes - Parler à un agent

1. Dans la barre latérale, ouvrez **Agents**.

2. Sur la ligne de l'agent, cliquez sur **Discuter**. Une conversation avec cet agent s'ouvre dans l'écran **Chat**.

3. Écrivez votre demande et envoyez-la. L'agent répond dans la conversation, et les étapes de son travail s'affichent au fur et à mesure.

Vous pouvez aussi cliquer sur la ligne pour ouvrir la fiche de l'agent, puis sur **Nouveau chat** en haut de la fiche. Depuis l'écran **Chat**, le sélecteur de la conversation liste vos agents : en choisir un ouvre une nouvelle conversation avec lui.

## Étapes - Confier une série d'éléments à un agent

1. Ouvrez **Agents** et cliquez sur la ligne de l'agent pour ouvrir sa fiche.

2. Cliquez sur **Traiter un lot** en haut de la fiche. La fenêtre qui s'ouvre confie les éléments d'une source à l'agent un par un, chaque élément étant une demande à part entière.

3. Sous **Source**, choisissez d'où viennent les éléments :
   - **Fichier Excel ou CSV** : une ligne du classeur devient un élément. **Choisir un fichier…** le prend sur ce poste. Un aperçu du classeur permet de choisir la feuille, la ligne d'en-tête et les colonnes envoyées à l'agent.
   - **Dossier** : un fichier du dossier devient un élément. Prenez un dossier connecté, ou **Choisir un dossier…** sur ce poste. L'agent reçoit le texte de chaque fichier, lu dans les fichiers PDF, Word, PowerPoint, les classeurs, les CSV et le texte brut. Cochez **Lire aussi les sous-dossiers** pour prendre aussi les fichiers de ses sous-dossiers. Le dossier est seulement lu : rien n'y est écrit.
   - **Boîte mail** : un message reçu devient un élément. La liste contient les boîtes IMAP et le compte Microsoft (lu comme Outlook) de vos **Connexions**. **Ajouter une boîte mail…** ouvre **Connexions**. Cochez **Messages non lus seulement** pour laisser de côté les messages déjà lus.

4. Sous **Ce que l'agent fait pour chaque élément**, écrivez la consigne envoyée avec chaque élément.

5. Sous **Colonnes du résultat**, donnez un nom à chaque colonne et dites ce que contient le champ. **Ajouter une colonne** en ajoute une. L'agent répond à chaque élément avec exactement ces champs, et chaque champ devient une colonne. Pour une source fichier, les colonnes sont écrites dans une copie du classeur. Avec un classeur .xlsx ou .xlsm, vous pouvez choisir à la place **Dans le fichier d'origine, après une sauvegarde**. Pour un dossier ou une boîte mail, les résultats sont gardés avec le lot, et vous les exportez depuis sa vue.

6. Sous **À la fin du lot (facultatif)**, dites ce que l'agent écrit une fois tous les éléments traités, par exemple un classement ou une recommandation. L'agent écrit cette synthèse à partir du tableau des résultats. Elle s'affiche sur la vue du lot et, pour une source fichier ou dossier, elle est écrite en fichier Markdown à côté de la source, nommée dans la langue de l'interface : `<nom> (synthèse AAAA-MM-JJ).md` en français.

7. Sous **Accès à accorder pour la durée du lot**, cochez les accès sensibles que l'agent peut utiliser sans vous demander, jusqu'à la fin du lot. Sans accord, l'élément concerné attend votre décision dans **À valider**, et le lot continue.

8. Sous **Seuil d'échecs**, choisissez après combien d'échecs le lot s'interrompt de lui-même, ou **Ne jamais interrompre**.

9. Cliquez sur **Lancer le lot**. La vue du lot s'ouvre et le suit élément par élément. Voir [La vue d'un lot](../screens/batch.md).

Tous les lots restent dans **Historique des lots**, en haut de l'écran **Agents**. Un fichier Excel ou CSV déposé dans le Chat propose aussi **Traiter chaque ligne**, qui ouvre la même fenêtre.

## Étapes - Laisser une automatisation appeler l'agent

1. Ouvrez la fiche de l'agent et cliquez sur **Automatiser**. La fenêtre qui s'ouvre est préremplie avec cet agent comme cible.

2. Dites quand l'automatisation doit se lancer, puis créez-la. Elle figure dans l'écran **Automatisations**. Voir [Programmer un trigger](../automatisations/schedule-a-trigger.md).

## Étapes - Lancer un workflow

1. Dans la barre latérale, ouvrez **Agents**. Les workflows figurent dans la colonne **Workflows**.

2. Sur la ligne du workflow, cliquez sur **Lancer**.

3. Remplissez les entrées que le workflow demande, puis lancez-le. Sa fiche montre où en est chaque étape de l'exécution.

## Choisir le palier d'autonomie d'une exécution en ligne de commande

En ligne de commande, une exécution prend un palier d'autonomie avec l'option `--autonomy` :

```
apollia-os run <id-agent> "<votre demande>" --autonomy <palier>
```

L'identifiant de l'agent est obligatoire. Le texte de la demande est facultatif et vaut
vide par défaut, ce qui est rarement ce que vous voulez.

Les quatre paliers disponibles :

| Palier | Comportement |
|---|---|
| `assisted` | La barrière de plan est armée : vous approuvez le plan une fois avant son exécution. Aucune passe de vérification. |
| `supervised` | Barrière de plan armée, plus une passe de vérification après la fin de l'exécution. |
| `bounded_autonomous` | **La barrière de plan est contournée** : le plan s'exécute sans votre approbation. Budget d'étapes plus large, une passe de vérification à la fin. Passez `--plan` pour réarmer la barrière sur une exécution. |
| `long_autonomous` | Même contournement que ci-dessus, budget maximal. Réservé aux tâches qui tolèrent de tourner sans surveillance de bout en bout. |

À chaque palier, une écriture sur le système de fichiers que le runtime juge risquée déclenche sa propre
demande d'approbation, indépendamment de la barrière de plan.

Si vous omettez l'option, l'exécution prend le palier de l'agent, et `assisted` quand l'agent n'en nomme aucun.

> Pour le détail des paliers et de leurs garanties, voir [Paliers d'autonomie](choose-an-autonomy-level.md).

## Vérification

- **Conversation** : votre message reçoit une réponse qui arrive progressivement.
- **Fiche de l'agent** : l'onglet **Activité** liste l'exécution sous **Exécutions**, avec son entrée, sa sortie et ses étapes.

## Si ça ne marche pas

- **La ligne n'a pas de bouton Discuter, et Nouveau chat est grisé :** l'agent est **indisponible**. Rendez-le de nouveau disponible avec `apollia-os agent start <id-agent>`.
- **L'agent porte la mention « dégradé » :** il prend encore une tâche. Ouvrez ses logs pour lire ce qui s'est mal passé, voir [Consulter les logs d'un agent](view-an-agent-logs.md).
- **Aucune réponse n'arrive :** vérifiez dans l'écran **IA** qu'un modèle est installé ou que votre serveur répond.
- **L'agent demande votre accord et attend :** répondez dans la conversation, ou dans l'écran **À valider**.
- **L'agent s'arrête trop tôt :** le budget d'étapes du palier est atteint. Voir [Paliers d'autonomie](choose-an-autonomy-level.md).
- **L'agent prend la tâche mais ne termine jamais :** voir [Un agent est bloqué](../troubleshooting/an-agent-is-stuck.md).

> **Concept :** [Explication Apollia](/explanation) - ce que sont un agent, un workflow et un paquet.
