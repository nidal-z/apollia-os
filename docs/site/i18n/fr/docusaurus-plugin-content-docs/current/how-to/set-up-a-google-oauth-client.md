---
sidebar_position: 6.6
title: Configurer un client OAuth Google
---

# Configurer un client OAuth Google

Le connecteur Google d'Apollia a besoin d'un client OAuth qui vous
appartient. Ce guide en crée un à partir de rien, nomme chaque écran et
chaque bouton, et se termine avec Gmail, Calendar et Drive accessibles
depuis Apollia.

Prévoyez dix à quinze minutes. Rien ici ne coûte d'argent, et rien
n'exige un abonnement Google Workspace : un compte personnel `@gmail.com`
suffit.

Si vous voulez seulement la version courte, les mêmes étapes sont
condensées sur
[Connecter Google Workspace](/operator-help/integrations/connect-google-workspace).

:::note Microsoft 365 n'a besoin de rien de tout cela
Microsoft se connecte sans rien à configurer, parce qu'Apollia embarque
l'identifiant de sa propre application enregistrée. Google n'autorise pas
l'équivalent.
[Pourquoi les deux diffèrent](#pourquoi-google-le-demande-et-pas-microsoft)
se trouve en fin de page.
:::

## Avant de commencer

- Un compte Google. Personnel ou Workspace, les deux fonctionnent.
- Apollia installé et lancé.
- Un navigateur connecté à ce compte Google.

Vous allez naviguer entre deux fenêtres : la **console Google Cloud**
dans votre navigateur, et **Apollia**. La dernière section est la seule
qui se déroule dans Apollia.

## Étape 1. Créer un projet Google Cloud

Un projet est un conteneur pour le client que vous allez créer. C'est
gratuit, et cela n'existe que pour donner à Google un endroit où rattacher
le client.

1. Ouvrez [console.cloud.google.com](https://console.cloud.google.com) et
   connectez-vous.
2. Si c'est votre première visite, acceptez les conditions d'utilisation.
   On pourra vous demander un pays et si vous souhaitez recevoir des
   actualités par e-mail. Aucune carte bancaire n'est requise.
3. Dans la barre bleue en haut, cliquez sur le **sélecteur de projet**, le
   menu déroulant juste à droite du logo « Google Cloud ». Il affiche
   *Sélectionner un projet*, ou le nom d'un projet que vous possédez
   déjà.
4. Dans la fenêtre qui s'ouvre, cliquez sur **Nouveau projet** en haut à
   droite.
5. Sous **Nom du projet**, saisissez quelque chose que vous reconnaîtrez
   plus tard, par exemple `Apollia`. Laissez **Emplacement** tel quel.
6. Cliquez sur **Créer**. Une notification apparaît après quelques
   secondes.
7. Rouvrez le sélecteur de projet et cliquez sur votre nouveau projet,
   pour que la barre bleue affiche son nom. Tout ce qui suit s'applique au
   projet sélectionné, et se tromper de projet ici est la façon la plus
   courante de se retrouver perdu trois étapes plus loin.

## Étape 2. Activer les API que vous voulez utiliser

Un projet tout neuf ne peut rien appeler. Vous activez une API par
service Google que vous voulez qu'Apollia puisse atteindre.

1. Dans la barre de recherche en haut, tapez `Gmail API` et sélectionnez
   le résultat **Gmail API** sous *Marketplace*.
2. Cliquez sur **Activer**. Attendez que la page se transforme en tableau
   de bord de l'API.
3. Répétez pour chaque service voulu :
   - **Google Calendar API** pour les événements de calendrier.
   - **Google Drive API** pour les fichiers Drive.
   - **Google Sheets API**, **Google Docs API**, **Google Slides API**,
     **Google Forms API**, **Google Tasks API**, **YouTube Data API v3**
     si vous les voulez aussi.

Activer une API que vous n'utilisez jamais ne coûte rien. En oublier une
se traduit plus tard par une erreur de permission quand un agent appelle
ce service, donc autant activer dès maintenant Gmail, Calendar et Drive.

## Étape 3. Configurer l'écran de consentement

L'écran de consentement est ce que vous verrez au moment de connecter
votre compte : la page qui liste ce qu'Apollia demande. Google exige
qu'il existe avant de délivrer un client.

1. Dans la navigation à gauche, ouvrez **APIs & Services**, puis **OAuth
   consent screen**. Si le menu est masqué, cliquez sur l'icône
   hamburger en haut à gauche.
2. Google ouvre alors un court formulaire de présentation. Renseignez :
   - **App name** : `Apollia`, ou tout autre nom de votre choix. C'est le
     nom que vous verrez sur la page de consentement.
   - **User support email** : choisissez votre propre adresse dans le
     menu déroulant.
   - **Developer contact information** : votre adresse à nouveau, en bas
     du formulaire.
3. Pour **Audience**, choisissez **External**. *Internal* n'est proposé
   que sur un compte Workspace et restreint le client à votre
   organisation ; **External** est la bonne réponse pour un compte
   personnel et fonctionne tout aussi bien pour un compte Workspace.
4. Cliquez sur **Create** (ou **Save and continue** à travers les
   sections restantes, selon la version de la console que vous obtenez).
   Vous pouvez laisser la section des scopes vide : Apollia demande ce
   dont il a besoin au moment de la connexion.

### S'ajouter comme utilisateur de test

Un écran de consentement tout neuf est en statut **Testing**, ce qui
signifie que seules les adresses que vous listez explicitement peuvent
l'utiliser.

1. Toujours sous **OAuth consent screen**, ouvrez la section
   **Audience**.
2. Sous **Test users**, cliquez sur **Add users**.
3. Saisissez l'adresse Google que vous comptez connecter à Apollia,
   appuyez sur Entrée, puis cliquez sur **Save**.

Sauter cette étape fait échouer la connexion à la page de consentement,
avec un message indiquant que l'application n'a pas terminé sa
vérification.

## Étape 4. Créer le client OAuth

1. Dans la navigation à gauche, sous **APIs & Services**, ouvrez
   **Credentials**.
2. Cliquez sur **+ Create credentials** en haut, puis **OAuth client
   ID**.
3. Sous **Application type**, choisissez **Desktop app**. C'est le choix
   qui compte : c'est ce type qui autorise la redirection en boucle
   locale (loopback) sur laquelle Apollia écoute. Un client de type *Web
   application* sera rejeté plus loin.
4. Sous **Name**, saisissez `Apollia desktop` ou tout autre nom. Ce nom
   est interne à la console et ne vous sera plus jamais montré.
5. Cliquez sur **Create**.

Une fenêtre apparaît avec votre **Client ID** et votre **Client
secret**.

6. Cliquez sur **Download JSON** et gardez le fichier dans un endroit où
   vous pourrez le retrouver dans une minute. C'est le chemin le plus
   rapide vers Apollia, qui lit les deux valeurs directement depuis ce
   fichier.

Si vous fermez la fenêtre trop tôt, le fichier reste disponible : sur la
page **Credentials**, cliquez sur l'icône de téléchargement à droite de
la ligne de votre client.

:::caution Le client secret n'est pas un mot de passe à partager
Google délivre un `client_secret` pour un client Desktop et l'exige lors
de l'échange du code d'autorisation, même si Apollia utilise aussi PKCE.
La documentation de Google elle-même indique que cette valeur n'est pas
traitée comme confidentielle pour les applications installées. Elle vous
reste néanmoins propre : gardez le fichier JSON pour vous, et ne le
commitez jamais dans un dépôt.
:::

## Étape 5. Transmettre le client à Apollia

1. Dans Apollia, ouvrez **Paramètres**, puis **Intégrations OAuth**.
2. Sur la carte **Google Workspace**, cliquez sur **Importer un JSON** et
   sélectionnez le fichier téléchargé. Le client ID et le client secret
   en sont extraits puis écrits dans `~/.apollia/oauth-clients.toml`, un
   fichier lisible par votre utilisateur uniquement.
3. Cliquez sur **Tester la configuration**. Le résultat attendu confirme
   que le client est présent, bien formé, et que le serveur
   d'autorisation de Google est joignable.

Si vous préférez saisir les deux valeurs à la main, les champs de la même
carte les acceptent directement. Le client ID se termine par
`.apps.googleusercontent.com` et le secret commence par `GOCSPX-`.

Vous avez terminé dans la console. Pour connecter un compte, allez dans
**Connexions**, sélectionnez **Google Workspace**, et suivez
[Connecter Google Workspace](/operator-help/integrations/connect-google-workspace).

## Ce que coûte le statut Testing

Laisser l'écran de consentement en **Testing** est gratuit et immédiat.
Cela impose deux limites, et la seconde surprend souvent :

- **100 utilisateurs de test au maximum.** Sans conséquence pour un usage
  personnel.
- **Les jetons de rafraîchissement expirent au bout de sept jours.**
  Apollia vous demandera de reconnecter le compte environ une fois par
  semaine. Rien n'est perdu quand cela arrive, il suffit de repasser par
  la page de consentement.

Passer l'écran de consentement en **Production** sans vérification
supprime l'expiration à sept jours, au prix d'un avertissement « Google
n'a pas vérifié cette application » qu'il faut déplier et valider une
fois par connexion. Pour se débarrasser des deux, il faut soumettre
l'écran à vérification : c'est gratuit pour les scopes demandés par
défaut par Apollia, et cela prend plusieurs semaines.

Les scopes restreints (`gmail.readonly`, `gmail.modify`, `gmail.compose`,
et l'accès complet à Drive) relèvent d'un cas à part. Ils exigent une
évaluation de sécurité CASA de niveau 2, facturée par un tiers agréé par
Google, et c'est pour cette raison qu'Apollia ne les demande pas par
défaut.

## Pourquoi Google le demande et pas Microsoft

Les deux fournisseurs ne tracent pas la limite au même endroit.

Microsoft traite une application desktop comme un **client public** :
elle ne détient aucun secret, prouve chaque requête avec PKCE, et son
identifiant d'application est un GUID public que n'importe qui peut lire
dans le binaire. Apollia peut donc enregistrer une seule application et
en embarquer l'identifiant, ce qui explique que Microsoft 365 se
connecte sans rien à configurer.

Google exige deux choses qu'Apollia ne peut pas satisfaire à votre
place. Son écran de consentement doit passer une vérification avant que
l'application puisse servir des comptes en dehors de son propre projet,
et son type de client Desktop exige un client secret au niveau de son
point de terminaison de jetons, ce qu'aucun binaire distribué ne peut
garder secret. Un client Apollia unique et partagé placerait aussi tous
les utilisateurs derrière un même quota et un même écran de consentement.

Les dix minutes ci-dessus vous achètent donc une identité qui vous
appartient, avec votre propre quota, que l'usage de personne d'autre ne
peut ralentir ou faire suspendre.

## En cas d'échec

- **« Access blocked : Apollia has not completed the Google verification
  process »** : l'adresse que vous connectez n'est pas dans la liste des
  utilisateurs de test. Revenez à l'étape 3 et ajoutez-la.
- **« Error 400 : redirect_uri_mismatch »** : le client a été créé en
  tant qu'application *Web application* plutôt qu'en **Desktop app**.
  Créez un nouveau client avec le bon type ; il est impossible de
  changer le type d'un client existant.
- **Apollia refuse de se connecter et signale un secret manquant** : le
  client ID a été saisi à la main sans son secret. Importez plutôt le
  fichier JSON, ou collez le secret dans le second champ.
- **Un agent obtient une erreur de permission sur un seul service** :
  l'API de ce service n'a jamais été activée à l'étape 2. Activez-la et
  réessayez, aucune reconnexion n'est nécessaire.
- **On vous demande de vous reconnecter chaque semaine** : comportement
  attendu en statut Testing, voir ci-dessus.
