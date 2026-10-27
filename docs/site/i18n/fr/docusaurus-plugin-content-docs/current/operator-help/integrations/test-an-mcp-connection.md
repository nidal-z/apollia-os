---
title: Vérifier une connexion et la réparer
slug: /operator-help/integrations/test-an-mcp-connection
sidebar_position: 6
---

# Vérifier une connexion et la réparer

> Pour tout opérateur qui veut savoir si une connexion fonctionne vraiment, et la remettre d'aplomb quand elle ne fonctionne pas.

## Prérequis

- Au moins une connexion dans **Connexions** : un compte, une boîte mail, un dossier ou un service.

## Vérifier une connexion

1. Dans **Connexions**, ouvrez la fiche de la connexion.
2. Cliquez sur **Vérifier**. Apollia essaie la connexion tout de suite : il se connecte au compte, ouvre la boîte mail, cherche le dossier, ou démarre le service et lui demande ses outils. Un service qui dormait est démarré pour la vérification.
3. L'état en haut de la fiche montre ce qui a été constaté, avec l'heure : *vérifiée à l'instant*.

Pour un service, son usage met aussi son état à jour : quand ses appels échouent il passe **À réparer**, et quand ils marchent de nouveau il revient à **Fonctionne**. La page l'apprend aussitôt, sans rien recharger.

## Lire l'état

| État | Ce qu'il veut dire |
|---|---|
| **Fonctionne** | La dernière vérification l'a trouvée en marche, ou pour un service, son dernier usage. |
| **À réparer** | Il faut d'abord faire quelque chose. La raison dit quoi, en clair. |
| **En pause** | Vous l'avez mise en pause. Voir [Mettre en pause ou retirer une connexion](pause-or-remove-a-connection.md). |
| **Bloquée par l'organisation** | La politique de votre organisation l'interdit, ou un administrateur doit d'abord l'approuver. |
| **Pas encore vérifiée** | Rien ne l'a essayée depuis son ajout. Elle est rangée avec les autres, pas en tête. |

Un état n'est **Fonctionne** qu'une fois qu'Apollia l'a vue fonctionner. Avec le mode développeur activé, la section **Détail technique** de la fiche montre aussi la raison brute de l'état.

## La réparer

La fiche propose, sous l'état, la seule action qui répare la connexion :

| Raison | Action | Ce qu'elle fait |
|---|---|---|
| *le compte doit se reconnecter* | **Se reconnecter** | Ouvre la connexion au compte, comme à l'ajout. Un service distant qui se connecte par sa propre page fait de même. |
| *un administrateur doit d'abord l'approuver* (Microsoft 365) | **Réessayer** | La fiche montre l'adresse d'accord et une demande écrite pour votre administrateur, chacune prête à copier (**Copier l'adresse**, **Copier la demande**). Une fois l'accord donné par votre administrateur, **Réessayer** rouvre la connexion au compte. |
| *le serveur refuse le mot de passe* | **Saisir le mot de passe** | Ouvre le formulaire de la boîte mail sur cette adresse, pour un nouveau mot de passe d'application. |
| *le dossier n'est plus à sa place* | **Choisir le dossier** | Vous fait choisir le dossier à nouveau. La connexion reste la même, donc ses agents gardent leurs accès. |
| *injoignable* ou *ses opérations échouent* | **Relancer** | Redémarre le service. |
| Aucune affichée, et aucune adresse d'accord : l'état indique **Bloquée par l'organisation** | **Voir la règle** | Ouvre **Autorisations**. |

Une fois le dossier choisi à nouveau, les agents cherchent aussitôt dans son nouveau contenu : rien de l'ancien dossier ne revient dans leurs réponses, même pendant que le nouveau est encore en cours de lecture.

Pour un compte Microsoft professionnel qu'un administrateur doit approuver, voir l'entrée **Bloquée par votre organisation** de [Connecter Microsoft 365](connecter-microsoft-365.md).

## Messages d'erreur à l'ajout d'un service

Quand vous ajoutez un service et que son essai échoue, Apollia traduit l'erreur technique en message clair :

- *« Authentification refusée - vérifiez votre clé API. »* : clé invalide ou expirée.
- *« Accès interdit - votre clé n'a pas les droits requis. »* : il manque une permission à la clé.
- *« Service introuvable - vérifiez l'URL ou le nom du serveur. »* : mauvaise adresse ou serveur absent.
- *« Erreur réseau - le service n'a pas répondu à temps. »* : le service n'a pas pu être joint.
- *« Commande introuvable - le paquet n'est probablement pas installé. »* : un service local dont le programme manque.
- *« La connexion a échoué. Vérifiez vos identifiants et réessayez. »* : tout autre échec.

**Voir le détail technique** montre le message brut, **Réessayer** essaie à nouveau et **Retirer** enlève la connexion.

## Vérification

- L'état indique **Fonctionne**, avec une heure récente.
- Les outils attendus sont listés sur la fiche.
- S'il manque des outils à un service, regardez d'abord Apollia : il garde au plus `max_tools` outils par serveur, 256 par défaut, et journalise `mcp.tools.bounded` avec ce qu'il a gardé et ce qu'il a reçu quand il coupe. Une réponse de serveur est aussi plafonnée à `max_response_bytes`. Ce n'est qu'une fois ces deux causes écartées que le service lui-même devient l'explication.

## En cas de problème

- **L'état reste À réparer après l'action** : cliquez de nouveau sur **Vérifier**, puis lisez la raison. Avec le mode développeur activé, **Détail technique** donne le message brut.
- **Un service local est injoignable** : son programme n'est pas dans le PATH d'Apollia, ou il s'arrête aussitôt. Vérifiez la commande avec le mode développeur activé.
- **Elle fonctionne, mais un agent ne s'en sert pas** : l'agent ne tient pas ses outils. Sur la fiche, **Donner à un agent**, ou voir [Comprendre la portée d'une intégration](understand-integration-scope.md).

> **Référence technique :** [Référence Apollia](/reference) , états des connexions, sémantique de la poignée de main MCP.
