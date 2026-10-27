---
title: L'écran Connexions
slug: /operator-help/screens/connections
sidebar_position: 4
---

# L'écran Connexions

> Ce qui relie vos agents à vos comptes, boîtes mail, dossiers et services, et si chacun fonctionne.

## À quoi ça sert

Une connexion donne aux agents des outils pour travailler avec ce qui est à vous : un compte Microsoft 365 ou Google Workspace, une boîte mail, un dossier de votre ordinateur, ou un service du catalogue. Les outils fournis avec Apollia figurent ici aussi, sous **Apollia, outils intégrés**.

L'écran a deux parties côte à côte : la liste à gauche, la fiche de la connexion choisie à droite. Apollia garde votre place, la connexion choisie, la recherche et le filtre, quand vous quittez l'écran et y revenez.

## La liste

- **Rechercher une connexion…** filtre la liste à mesure que vous tapez.
- Le filtre de genre la réduit aux **Comptes**, **Boîtes**, **Services** ou **Dossiers**, ou montre **Toutes**.
- Ce qui est **À réparer** vient en tête, puis tout le reste par nom.
- Chaque compte connecté a sa propre ligne : deux comptes Microsoft font deux lignes.
- Une connexion installée par un paquet porte **Apportée par le paquet** suivi du nom du paquet.

Chaque ligne porte l'état de sa connexion :

| État | Ce qu'il veut dire | L'action sur sa fiche |
|---|---|---|
| **Fonctionne** | La dernière vérification l'a trouvée en marche, ou pour un service, son dernier usage. | Aucune. |
| **À réparer** | Il faut d'abord faire quelque chose. La raison est écrite en clair, par exemple *le compte doit se reconnecter* ou *le dossier n'est plus à sa place*. | La seule action qui la répare : **Se reconnecter**, **Saisir le mot de passe**, **Choisir le dossier** ou **Relancer**. |
| **En pause** | Vous l'avez mise en pause. Ses agents gardent leurs accès, et aucun de ses outils ne s'exécute. | **Reprendre**. |
| **Bloquée par l'organisation** | La politique de votre organisation l'interdit, ou un administrateur doit d'abord l'approuver. | **Voir la règle**. |
| **Pas encore vérifiée** | Rien ne l'a essayée depuis son ajout. | **Vérifier**. |

Un état constaté par une vérification ou un usage dit quand il l'a été, par exemple *vérifiée il y a 5 min*. **En pause**, **Pas encore vérifiée** et un blocage qui attend l'accord d'un administrateur ne portent pas d'heure. Un état change à l'écran dès qu'Apollia le constate, sans recharger la page.

## La fiche

- **Vérifier** essaie la connexion tout de suite et montre ce qui a été constaté.
- **Essayer** ouvre une conversation avec Apollia limitée aux outils de cette connexion.
- **Mettre en pause** et **Reprendre**, puis **Retirer**, qui liste ce qui part avec la connexion avant que vous confirmiez. Voir [Mettre en pause ou retirer une connexion](/operator-help/integrations/pause-or-remove-a-connection).
- **Agents qui s'en servent** liste les agents qui tiennent ses outils, **Par défaut** marquant ceux qui la tiennent par l'accès que reçoit tout nouvel agent. **Donner à un agent** la donne à plusieurs agents d'un coup.
- **Activité sur 7 jours** : sa dernière utilisation et le nombre d'actions de chaque agent avec elle. **Voir dans le Journal** ouvre le **Journal** sur cette seule connexion. Une boîte mail partage ses outils avec toutes les autres, son activité n'est donc pas comptée à part.
- Les outils de la connexion, chacun avec sa famille (lecture, écriture ou suppression, envoi vers l'extérieur, etc.). Pour un service ajouté hors de la sélection d'Apollia, **Changer** permet de donner une autre famille à un outil.
- Pour un service, ses réglages, comme ses arguments, ses secrets et ses délais. Sa façon d'être lancé apparaît avec le mode développeur activé.
- Pour un compte Microsoft, les dossiers et fichiers OneDrive qu'Apollia peut lire. Pour un compte Google, le dossier Drive où travaillent ses agents. Pour un compte, **Ajouter un autre compte** en connecte un second du même fournisseur.

Avec le mode développeur activé, une section **Détail technique** ajoute l'identifiant de la connexion, sa provenance et la raison brute de son état, et la fiche d'un compte montre le client OAuth qu'il utilise.

## Ajouter une connexion

**Ajouter une connexion** ouvre une seule fenêtre avec une seule recherche. Les réponses viennent en quatre groupes : **Proposés par Apollia** (les comptes et les services sélectionnés par Apollia), **Sur cet ordinateur** (un dossier, une boîte mail), le **Catalogue public**, interrogé seulement quand vous cliquez sur **Explorer tout** et marqué **Non vérifié**, et **Brancher un service par son adresse**. Chaque ajout suit ensuite les trois mêmes étapes : **Brancher**, **Qui peut s'en servir**, **Essai**. Voir [Connecter un serveur MCP du catalogue](/operator-help/integrations/connect-an-mcp-server).

## Pour aller plus loin

- [Vue d'ensemble des intégrations](/operator-help/integrations/integrations-overview)
- [Vérifier une connexion et la réparer](/operator-help/integrations/test-an-mcp-connection)
- [Mettre en pause ou retirer une connexion](/operator-help/integrations/pause-or-remove-a-connection)
- [Connecter Microsoft 365](/operator-help/integrations/connect-microsoft-365)
