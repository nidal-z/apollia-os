# Gérer mon profil

> Pour les operators qui veulent **consulter et modifier** au quotidien ce que tous leurs agents savent d'eux - prénom, rôle, secteur, supervision, contraintes.
>
> Cette page couvre **l'édition d'un profil existant**. Si vous lancez Apollia pour la première fois, le parcours guidé peuple les valeurs initiales : voir **[Configurer votre profil au premier lancement](../installation/configurer-votre-profil.md)**.

## Pourquoi un profil utilisateur

Tous vos agents partagent un **profil utilisateur** unique. Quand vous dites à l'un d'eux que vous êtes développeur·euse en fintech, les autres agents profitent de cette information : vocabulaire adapté, ton ajusté, suggestions pertinentes. Pas de re-saisie à chaque agent.

Ce profil est **local**. Aucune donnée ne quitte votre machine.

## Où l'éditer

**Paramètres → Profil**, accessible depuis l'icône ⚙️ de la sidebar.

![Page Parametres puis Profil, avec ses sections empilees, de l'identite jusqu'a la zone de danger](/img/operator-help/fr/memoire-gerer-mon-profil-1.png)

L'écran est divisé en sections :

- **Identité** - prénom, rôle, secteur, taille d'équipe, objectifs.
- **Supervision des agents** *(sensible)* - niveau HITL (Human-in-the-Loop), domaines d'autonomie, déclenchement.
- **Outils & contexte métier** - outils du quotidien, niveau technique, intégrations connectées *(la dernière est sensible)*.
- **Contraintes** *(sensible)* - souveraineté des données, conformité.
- **Préférences** - langue préférée, backend LLM par défaut.

## Étapes

1. Ouvrez **Paramètres → Profil**.

2. Saisissez ou modifiez les champs voulus. Chaque champ est sauvegardé automatiquement quand vous quittez le champ (sortie de focus) ou changez d'option. Un toast confirme l'enregistrement.

3. Vider un champ et quitter le focus **supprime** l'entrée du profil.

## Qui a renseigné chaque champ

Une **pastille** apparaît à côté des champs déjà renseignés et indique l'origine de la valeur :

- **onboarding** - valeur posée par le parcours guidé au premier démarrage. Vous la verrez sur les quelques champs que l'onboarding remplit (prénom, rôle, niveau de supervision, souveraineté des données).
- **vous** - valeur saisie ou modifiée depuis ce formulaire. Toute modification que vous faites ici remplace l'origine précédente par **vous**.
- **agent** - valeur déduite par un agent au fil d'une conversation (par exemple un agent qui aurait observé votre rôle dans un échange).

Un champ vide ne porte pas de pastille. Tant que vous ne touchez pas à un champ, la pastille d'origine est préservée - passer le focus dessus sans rien changer ne la remplace pas.

## Champs sensibles

Quatre champs portent un badge **Sensible** :

- **Niveau HITL** (Supervision des agents)
- **Souveraineté des données** (Contraintes)
- **Conformité** (Contraintes)
- **Intégrations connectées** (Outils & contexte métier)

Modifier ces champs **ne ré-applique pas automatiquement** vos règles de permissions. Apollia respecte le principe « la mémoire ne change pas l'environnement sans décision explicite ». Pour que les nouvelles valeurs influencent les permissions, relancez l'onboarding (voir ci-dessous).

## Réinitialiser votre profil

Plusieurs chemins, selon ce que vous voulez effacer.

### A - Effacer uniquement le profil et reposer les questions

Au bas de la page **Paramètres → Profil**, la **Zone danger** propose un bouton **Réinitialiser le profil**. Confirmez dans la modale.

![La zone de danger du profil avec la modale de confirmation Réinitialiser le profil au premier plan](/img/operator-help/fr/memoire-gerer-mon-profil-1bis.png)

- Tout le profil est effacé (les 5 sections).
- L'agent d'onboarding redémarre **immédiatement** pour reconstruire vos préférences depuis zéro.
- L'historique des conversations et les mémoires des autres agents **ne sont pas touchés**.

C'est l'option à choisir si vous voulez juste « repasser le questionnaire de configuration » sans toucher au reste.

### B - Repasser uniquement le parcours guidé (sans effacer le profil)

Dans **Paramètres → Zone de danger** (entrée distincte de la sidebar Paramètres), le bouton **Réinitialiser l'onboarding** relance le parcours guidé sans effacer ce qui est déjà renseigné. Utile si vous voulez juste re-télécharger un modèle, recalibrer une intégration ou voir les écrans de bienvenue à nouveau.

Il a un effet de bord que la fenêtre ne mentionne pas : il remet aussi les visites guidées à zéro. Les six parcours repartent de leur première étape et la bande **Prise en main** réapparaît sur le tableau de bord, alors que la fenêtre annonce qu'aucune donnée n'est supprimée.

### C - Effacer toutes les mémoires (profil + agents + projets)

Dans **Paramètres → Zone de danger**, le bouton **Effacer les mémoires** supprime **toutes** les mémoires Apollia, sur tous les namespaces : profil utilisateur, mémoire des agents, mémoire de projet. C'est plus large que la réinitialisation du profil - à utiliser si vous voulez repartir d'une page complètement blanche côté mémoire (les conversations, agents installés et permissions restent en place).

> Pour aller encore plus loin (effacer aussi les agents installés, les permissions, les paramètres système), c'est l'option **Réinitialisation usine** au bas de la même Zone de danger.

## Vérification

- Rechargez la page : les valeurs saisies persistent.
- Démarrez une nouvelle conversation avec n'importe quel agent et observez qu'il vous adresse par votre prénom / adapte son ton à votre rôle.
- Sur un champ que vous venez de modifier, la pastille bascule sur **vous**.

## Si ça ne marche pas

- **Les champs restent vides après saisie** : un agent était peut-être en train d'écrire. Réessayez après quelques secondes.
- **Le bouton « Réinitialiser le profil » ne relance pas l'onboarding** : utilisez **Réinitialiser l'onboarding** dans Paramètres → Zone de danger. Il n'y a pas d'entrée Onboarding dans la barre latérale.
- **Un agent ignore votre profil** : tous les agents Python ont accès au profil par défaut. Si l'agent en question est tiers, ouvrez son code (ou contactez son auteur) pour vérifier qu'il lit `ctx.profile`.

> **Référence technique :** [Référence Apollia](/reference) - schéma canonique des champs, source unique de vérité du profil global, contrat SDK Python `ctx.profile.*`.
