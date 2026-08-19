# Programmer un trigger

> Pour les operators qui veulent qu'une tâche IA s'exécute toute seule, à heure fixe ou sur événement, sans intervention manuelle.

## Prérequis

- Au moins un agent installé et démarrable depuis la page Mes assistants.
- Un fournisseur d'IA configuré (depuis **Paramètres → Modèles**).
- Vous savez à quelle fréquence vous voulez que la tâche se répète.

## Étapes - créer une automatisation en langage naturel (parcours par défaut)

1. Dans la sidebar, cliquez sur **Mes déclencheurs**. La page s'intitule **Automatisations**.

2. Cliquez sur le bouton **Créer une automatisation** en haut à droite. Un assistant en **4 étapes** s'ouvre (Décrire → Planifier → Assistant → Aperçu).
   ![Page Automatisations, bouton Creer une automatisation en haut a droite et stepper en quatre etapes](/img/operator-help/automatisations-programmer-un-trigger-1.png)

3. **Étape Décrire** - Décrivez **le quand** : à quel moment ou à quelle fréquence le déclencheur doit se déclencher. Exemples : *« Tous les matins à 8 h »*, *« Chaque lundi à 9 h pour préparer la semaine »*, *« Toutes les 30 minutes »*. Vous n'avez pas besoin de nommer un assistant à ce stade : un déclencheur est indépendant de l'assistant qui l'exécutera, vous choisirez cet assistant à l'étape **Assistant**. Cliquez sur **Suivant** ; Apollia analyse la phrase (l'étiquette du bouton passe à *« Analyse… »*).

4. **Étape Planifier** - Apollia affiche sa lecture de votre phrase dans un encadré (par exemple *« Tous les jours à 08:00 »*) avec la **prochaine exécution prévue**. Si quelque chose vous semble inexact, ajustez en langage naturel dans le champ du bas (*« plutôt à 9 h »*) et appuyez sur Entrée - la planification se met à jour. Si Apollia a besoin d'une précision sur le calendrier (heure manquante, jour ambigu…), un bandeau orange affiche les points à clarifier ; complétez-les via le champ d'ajustement. L'absence d'assistant dans la description n'est pas bloquante à cette étape.
   ![Etape Planification, encart de planification en clair, ligne de prochaine execution et champs d'affinage](/img/operator-help/automatisations-programmer-un-trigger-2.png)

5. **Étape Assistant** - Sélectionnez l'assistant qui exécutera ce déclencheur. Un déclencheur lance toujours **un assistant à la fois**. Si votre phrase nommait un agent existant, il est pré-sélectionné et un sous-texte indique *« Reconnu automatiquement : … »*. Sinon, un encart orange rappelle qu'aucun assistant n'a été reconnu, et vous le choisissez dans la liste déroulante. Seuls les assistants installés apparaissent.

6. **Étape Aperçu** - Apollia récapitule le déclencheur : la planification en clair, l'assistant cible, et la consigne transmise au déclenchement si elle a été détectée. Cliquez sur **Activer cette automatisation**.

7. Une notification confirme la création. L'automatisation apparaît dans la table avec un voyant **Active** vert et la colonne **Prochain déclenchement** indique la date prévue.

## Lancer manuellement et suivre

8. Pour vérifier que tout fonctionne sans attendre la prochaine échéance, passez la souris sur la ligne de l'automatisation et cliquez sur l'**icône lecture ▶︎** à droite. Une exécution démarre immédiatement et un toast confirme le lancement.
   A l'ecran : une ligne d'automatisation au survol, avec l'icône Play visible à droite et son infobulle Lancer maintenant.

9. Pour consulter l'historique d'exécution, cliquez sur l'icône **⋯** sur la ligne (visible au hover) → **Voir l'historique**. Voir la page [Suivre l'historique d'un trigger](suivre-l-historique-d-un-trigger.md).

## Modifier une automatisation existante

Une automatisation se modifie sur place. Changer un horaire ne passe plus par une suppression suivie d'une recréation, ce qui invalidait le secret du webhook et obligeait à reconfigurer le service appelant.

1. Sur la ligne de l'automatisation, cliquez sur le menu **⋯** → **Modifier**. Avec la ligne sélectionnée au clavier, la touche **E** fait la même chose.

2. La fenêtre **Modifier le trigger** s'ouvre sur la définition enregistrée, avec les mêmes champs qu'en mode avancé : assistant cible, type de déclencheur et ses paramètres, interrupteur **Activée**, comportement quand une exécution est déjà en cours, et modèle d'entrée.

3. **L'identifiant est grisé** : il adresse l'automatisation et ne peut plus changer après la création. Tout le reste est modifiable, y compris le type de déclencheur.

4. Pour un webhook, le secret enregistré n'arrive jamais dans la fenêtre. Le champ indique « *Secret déjà enregistré, laissez vide pour le conserver* » : laissez-le vide et le secret courant reste en place, remplissez-le et il est remplacé, ce qui oblige à reconfigurer le service appelant avec la nouvelle valeur.

5. Cliquez sur **Enregistrer**. Une confirmation nomme l'automatisation, la liste se recharge, et le moteur redémarre sur la nouvelle définition.

**Une automatisation déclarée dans `apollia.toml` ne se modifie pas ici.** La fenêtre le dit et s'arrête là : ces triggers tournent dans le moteur mais n'ont pas de définition enregistrée à réécrire. Modifiez le fichier, puis utilisez **Recharger la config**.

## Recharger le fichier de configuration

Le bouton **Recharger la config**, en haut à droite de la page, relit `apollia.toml` et redémarre le moteur d'automatisations sur cette base, puis annonce combien d'automatisations sont actives. Utilisez-le après avoir modifié le fichier à la main, cela évite de redémarrer l'application. Le bouton **Actualiser**, juste à côté, fait autre chose et beaucoup plus petit : il relit la liste que le moteur détient déjà.

## Mode avancé (optionnel)

Pour les opérateurs qui préfèrent saisir directement les paramètres techniques (expression cron exacte, chemin de fichier précis, secret webhook…), le wizard propose un lien **Mode avancé** en bas à gauche. Il ouvre une fenêtre de création détaillée où vous choisissez :

- **L'assistant cible** (en haut).
- **Le type de déclenchement** parmi cinq cartes :
  - **Sur un calendrier** - quotidien, hebdomadaire, ou à heure précise.
  - **À intervalle régulier** - toutes les 30 minutes, toutes les heures, etc.
  - **Une seule fois** - à une date et heure données.
  - **Quand un fichier ou un dossier change** - surveille un fichier précis ou un dossier (option **récursif** pour inclure les sous-dossiers).
  - **Via une URL externe** - déclenché par appel HTTP entrant (webhook).
- **Les paramètres du type choisi** - voir détails ci-dessous.
- Un interrupteur **Activée** (vrai par défaut).
- Une section **Paramètres avancés** repliée par défaut : nom technique (ID) personnalisable, comportement *« si un déclenchement est déjà en cours »* (mettre en file ou ignorer), et **consigne** envoyée à l'assistant.

### Paramètres par type - mode avancé

- **Sur un calendrier** : sélectionnez un preset (*Toutes les 15 min, 30 min, Toutes les heures, Quotidien, Hebdomadaire*) ou **Personnalisé** pour saisir une expression cron brute (`min heure jour mois jour-semaine`). Les presets **Quotidien** et **Hebdomadaire** affichent un sélecteur d'heure ; **Hebdomadaire** ajoute des puces pour choisir les jours. Au moins un jour est requis : si vous n'en cochez aucun, la planification n'est pas construite, et le formulaire l'indique sous les puces.
- **À intervalle régulier** : sélecteur d'unité + valeur (toutes les N secondes / minutes / heures).
- **Une seule fois** : un sélecteur de date + un sélecteur d'heure côte à côte.
- **Quand un fichier ou un dossier change** : un champ **Chemin surveillé** (acceptant un chemin de fichier précis **ou** un chemin de dossier), trois puces à activer/désactiver pour le type d'événement - **Création**, **Modification**, **Suppression** - et un interrupteur **Inclure les sous-dossiers**. Cet interrupteur active la surveillance récursive : tous les sous-dossiers (à n'importe quelle profondeur) sont alors également surveillés. Désactivé par défaut, il n'a d'effet que pour un chemin de dossier ; il est ignoré pour un fichier précis.
- **Via une URL externe** : un encart d'explication (l'URL de réception sera affichée après création) + un champ **Secret** avec un bouton **Générer** pour produire une clé aléatoire.

## Vérification

L'automatisation figure dans la table **Automatisations** avec :
- Un chip **Active** vert (point lumineux animé).
- Une colonne **Prochain déclenchement** indiquant la date/heure prévue.
- Une colonne **Dernière exéc** qui se remplit après le premier déclenchement.

## Si ça ne marche pas

- **L'assistant cible n'apparaît pas dans la liste** : il n'est pas installé. Allez sur **Mes assistants** et installez-le, puis rouvrez le wizard.
- **Apollia n'a pas compris ma phrase** : un message *« Nous n'avons pas pu comprendre automatiquement »* apparaît. Reformulez plus simplement en énonçant clairement la fréquence et l'heure (ex. *« Tous les jours à 9 h »*).
- **L'étape Planifier indique des points à clarifier** : l'encart orange en haut liste les ambiguïtés sur le calendrier (par exemple « heure manquante »). Tapez la précision dans le champ d'ajustement et validez avec Entrée. Si la seule chose qu'Apollia n'a pas trouvée est l'assistant, vous pouvez quand même passer à l'étape suivante : la sélection se fait à l'étape Assistant.
- **Le bouton "Activer" est désactivé** : il manque une donnée - vérifiez que l'étape Planifier n'a plus d'ambiguïté de calendrier et qu'un assistant est sélectionné.
- **Le lancement immédiat (icône ▶︎) est grisé** : l'automatisation est en pause. Reprenez-la depuis le menu **⋯** de la ligne, ou ouvrez **Modifier** et réactivez l'interrupteur **Activée**.
- **La fenêtre Modifier refuse d'ouvrir l'automatisation** : elle vient de `apollia.toml`. Changez-la dans le fichier, puis cliquez sur **Recharger la config**.

## Appeler un webhook depuis un service externe

Une automatisation de type **webhook** ne se déclenche pas toute seule : c'est un
service extérieur qui l'appelle. Apollia refuse l'appel s'il n'est pas signé, ce
qui évite que n'importe qui puisse déclencher votre agent en connaissant l'URL.

**L'adresse** est celle affichée à la création, de la forme :

```
POST http://127.0.0.1:7771/webhooks/<id-de-l-automatisation>
```

Notez qu'elle n'est **pas** sous `/api/v1`. Elle est en revanche derrière le même
jeton d'API que toutes les autres routes joignables en TCP : la couche de jeton
est posée sur le routeur entier et n'inspecte aucun chemin, donc la signature
s'ajoute au jeton au lieu de le remplacer. Un appel qui ne porte que la signature
reçoit un `401`.

L'adresse est donc joignable depuis un service que vous maîtrisez et configurez,
pas depuis un service qui sait seulement signer, comme un webhook GitHub. Deux
options : envoyer aussi le jeton, ou désactiver la couche avec
`require_token = false` sous `[api]`, ce qui retire l'authentification de toutes
les routes TCP et pas seulement de celle-ci.

**La signature** va dans l'en-tête `X-Apollia-Signature`, au format
`sha256=<hexadécimal>`. C'est le HMAC-SHA256 du **corps brut** de la requête,
octet pour octet, avec votre secret comme clé. Signer une version reformatée du
corps produit une signature invalide.

Exemple avec `curl` et `openssl` :

```sh
SECRET='votre-secret-de-32-caracteres-minimum'
BODY='{"source":"github","action":"push"}'
SIG=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" -r | cut -d' ' -f1)

curl -X POST http://127.0.0.1:7771/webhooks/<id-de-l-automatisation> \
  -H "Authorization: Bearer $(cat ~/.apollia/api-token)" \
  -H "X-Apollia-Signature: sha256=$SIG" \
  -H "Content-Type: application/json" \
  -d "$BODY"
```

**Les réponses possibles :**

| Code | Ce que ça veut dire |
|---|---|
| `200` | L'événement est accepté, l'automatisation part |
| `401` | Jeton d'API absent ou faux, ou en-tête `X-Apollia-Signature` absent, ou signature qui ne correspond pas |
| `404` | Aucune automatisation de type webhook avec cet identifiant |
| `503` | Le moteur d'automatisations n'est pas démarré |

Un `401` ne dit pas quelle cause s'applique, c'est volontaire. Vérifiez d'abord
le jeton, qui est rejeté avant même que la signature soit lue, puis vérifiez que
vous signez le corps brut et non une version réindentée.

> **Référence technique :** [Référence Apollia](/reference).
