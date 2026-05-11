# Mon profil

> Pour les operators qui veulent **consulter et modifier** au quotidien ce que tous leurs agents savent d'eux — prénom, rôle, secteur, supervision, contraintes.
>
> Cette page couvre **l'édition d'un profil existant**. Si vous lancez Apollia pour la première fois, le parcours guidé peuple les valeurs initiales : voir **[Configurer votre profil au premier lancement](../installation/configurer-votre-profil.md)**.

## Pourquoi un profil utilisateur

Tous vos agents partagent un **profil utilisateur** unique. Quand vous dites à l'un d'eux que vous êtes développeur·euse en fintech, les autres agents profitent de cette information : vocabulaire adapté, ton ajusté, suggestions pertinentes. Pas de re-saisie à chaque agent.

Ce profil est **local**. Aucune donnée ne quitte votre machine.

## Où l'éditer

**Paramètres → Profil**, accessible depuis l'icône ⚙️ de la sidebar.

L'écran est divisé en sections :

- **Identité** — prénom, rôle, secteur, taille d'équipe, objectifs.
- **Supervision des agents** *(sensible)* — niveau HITL (Human-in-the-Loop), domaines d'autonomie, déclenchement.
- **Outils & contexte métier** — outils du quotidien, niveau technique, intégrations connectées *(la dernière est sensible)*.
- **Contraintes** *(sensible)* — souveraineté des données, conformité.
- **Préférences** — langue préférée, backend LLM par défaut.

## Étapes

1. Ouvrez **Paramètres → Profil**.

2. Saisissez ou modifiez les champs voulus. Chaque champ est sauvegardé automatiquement quand vous quittez le champ (sortie de focus). Un toast confirme l'enregistrement.

3. Une pastille à côté de chaque champ indique **qui a renseigné la valeur** :
   - **onboarding** : extrait lors de l'agent d'onboarding au démarrage initial.
   - **vous** : saisi explicitement depuis ce formulaire.
   - **agent** : observé par un agent au fil d'une conversation.

4. Vider un champ et quitter le focus **supprime** l'entrée du profil.

## Champs sensibles

Quatre champs portent un badge **Sensible** :

- **Niveau HITL** (Supervision des agents)
- **Souveraineté des données** (Contraintes)
- **Conformité** (Contraintes)
- **Intégrations connectées** (Outils & contexte métier)

Modifier ces champs **ne ré-applique pas automatiquement** vos règles de permissions. Apollia respecte le principe « la mémoire ne change pas l'environnement sans décision explicite ». Pour que les nouvelles valeurs influencent les permissions, relancez l'onboarding (bouton **Réinitialiser le profil** ci-dessous, ou directement depuis la sidebar).

## Réinitialiser le profil

En bas de la page, la **Zone danger** propose un bouton **Réinitialiser le profil**. Confirmez dans la modale : tout le profil est effacé et l'agent d'onboarding redémarre pour reconstruire vos préférences depuis zéro.

L'historique des conversations et la mémoire des agents (épisodes, faits sémantiques) **ne sont pas touchés** par cette action — uniquement le profil global.

> Pour repasser le parcours guidé complet (re-télécharger un modèle, recalibrer les permissions à partir de vos nouvelles réponses), c'est plutôt l'option **Paramètres → Zone de danger → Réinitialiser l'onboarding** qu'il vous faut — voir [Relancer le parcours](../installation/configurer-votre-profil.md#relancer-le-parcours).

## Vérification

- Rechargez la page : les valeurs saisies persistent.
- Démarrez une nouvelle conversation avec n'importe quel agent et observez qu'il vous adresse par votre prénom / adapte son ton à votre rôle.

## Si ça ne marche pas

- **Les champs restent vides après saisie** : un agent était peut-être en train d'écrire. Réessayez après quelques secondes.
- **Le bouton « Réinitialiser le profil » ne relance pas l'onboarding** : ouvrez la sidebar et cliquez sur **Onboarding** pour relancer manuellement.
- **Un agent ignore votre profil** : tous les agents Python ont accès au profil par défaut. Si l'agent en question est tiers, ouvrez son code (ou contactez son auteur) pour vérifier qu'il lit `ctx.profile`.

> **Référence technique :** [Briques-User-Profile](https://github.com/nidal-z/apollia-os/wiki/Briques-User-Profile) — schéma canonique des champs, source unique de vérité du profil global, contrat SDK Python `ctx.profile.*`.
