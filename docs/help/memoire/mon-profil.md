# Mon profil

> Pour les operators qui veulent **consulter et modifier** au quotidien ce que tous leurs agents savent d'eux — prénom, rôle, secteur, supervision, contraintes.
>
> Cette page couvre **l'édition d'un profil existant**. Si vous lancez Apollia pour la première fois, le parcours guidé peuple les valeurs initiales : voir **[Configurer votre profil au premier lancement](../installation/configurer-votre-profil.md)**.

## Pourquoi un profil utilisateur

Tous vos agents partagent un **profil utilisateur** unique. Quand vous dites à l'un d'eux que vous êtes développeur·euse en fintech, les autres agents profitent de cette information : vocabulaire adapté, ton ajusté, suggestions pertinentes. Pas de re-saisie à chaque agent.

Ce profil est **local**. Aucune donnée ne quitte votre machine.

## Où l'éditer

**Paramètres → Profil**, accessible depuis l'icône ⚙️ de la sidebar.

`[SCREENSHOT: page Paramètres → Profil avec les 5 sections empilées (Identité, Supervision des agents, Outils & contexte métier, Contraintes, Préférences), badge "Sensible" en orange sur Supervision et Contraintes]`

L'écran est divisé en sections :

- **Identité** — prénom, rôle, secteur, taille d'équipe, objectifs.
- **Supervision des agents** *(sensible)* — niveau HITL (Human-in-the-Loop), domaines d'autonomie, déclenchement.
- **Outils & contexte métier** — outils du quotidien, niveau technique, intégrations connectées *(la dernière est sensible)*.
- **Contraintes** *(sensible)* — souveraineté des données, conformité.
- **Préférences** — langue préférée, backend LLM par défaut.

## Étapes

1. Ouvrez **Paramètres → Profil**.

2. Saisissez ou modifiez les champs voulus. Chaque champ est sauvegardé automatiquement quand vous quittez le champ (sortie de focus). Un toast confirme l'enregistrement.

   `[SCREENSHOT: section Identité avec les inputs Prénom / Rôle remplis, les selects Secteur / Taille d'équipe, et un toast vert "Profil mis à jour" en haut à droite]`

3. Une pastille à côté de chaque champ indique **qui a renseigné la valeur** :
   - **onboarding** : extrait lors de l'agent d'onboarding au démarrage initial.
   - **vous** : saisi explicitement depuis ce formulaire.
   - **agent** : observé par un agent au fil d'une conversation.

   `[SCREENSHOT: gros plan sur deux champs avec leurs badges source — badge bleu "onboarding" à côté de Prénom et badge vert "vous" à côté de Rôle]`

4. Vider un champ et quitter le focus **supprime** l'entrée du profil.

## Champs sensibles

Quatre champs portent un badge **Sensible** :

- **Niveau HITL** (Supervision des agents)
- **Souveraineté des données** (Contraintes)
- **Conformité** (Contraintes)
- **Intégrations connectées** (Outils & contexte métier)

`[SCREENSHOT: section "Supervision des agents" avec son badge "Sensible" orange, le RadioGroup HITL (Toujours valider / Critique seulement / Jamais) avec une option sélectionnée, et la mention "Modifier ces choix n'ajuste pas tes règles de permissions automatiquement"]`

Modifier ces champs **ne ré-applique pas automatiquement** vos règles de permissions. Apollia respecte le principe « la mémoire ne change pas l'environnement sans décision explicite ». Pour que les nouvelles valeurs influencent les permissions, relancez l'onboarding (bouton **Réinitialiser le profil** ci-dessous, ou directement depuis la sidebar).

## Réinitialiser le profil

En bas de la page, la **Zone danger** propose un bouton **Réinitialiser le profil**. Confirmez dans la modale : tout le profil est effacé et l'agent d'onboarding redémarre pour reconstruire vos préférences depuis zéro.

`[SCREENSHOT: Zone danger en bas de page avec icône AlertTriangle rouge, texte explicatif et bouton "Réinitialiser le profil" à bordure destructive]`

`[SCREENSHOT: modale de confirmation "Réinitialiser le profil ?" avec texte d'avertissement "Cela supprimera tout le profil mémorisé et relancera l'onboarding. Cette action est irréversible.", boutons "Réinitialiser" (rouge) et "Annuler"]`

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
