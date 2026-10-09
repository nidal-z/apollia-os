---
title: Tester une connexion MCP
slug: /operator-help/integrations/test-an-mcp-connection
sidebar_position: 6
---

# Tester une connexion MCP

> Pour tout operator qui veut vérifier qu'un serveur MCP installé répond bien, ou en diagnostiquer un en erreur.

## Prérequis

- Au moins un serveur MCP installé (voir [Connecter un serveur MCP](connecter-un-serveur-mcp.md)).

## Étapes

1. Dans **Connexions**, cliquez sur le serveur MCP à tester. Sa fiche s'ouvre à la place de la liste.

2. Dans l'en-tête de la fiche, cliquez sur **Tester**, à côté de **Reconnecter**.

   ![Fiche d'un serveur MCP installé, avec les boutons Tester et Reconnecter dans son en-tête](/img/operator-help/integration-tester-une-connexion-mcp-2.png)

3. Pendant le test, le bouton affiche **Test…** et se désactive.

4. Le résultat s'affiche sur une ligne dans l'onglet **Aperçu**. Elle porte un nombre d'outils, jamais une latence : rien sur le chemin MCP ne mesure un temps de réponse.

   - **Opérationnel - N outils, dernière opération réussie**, en vert.
   - **Joignable - N outils listés, pas encore vérifié par une opération**, en gris. Le serveur a répondu à la poignée de main, mais aucune opération ne l'a confirmé depuis.
   - **Attention requise - reconnecte ou vérifie les autorisations**, en orange. Avec le mode développeur activé, la ligne précise : **Joignable, mais des opérations récentes ont échoué** ou **Joignable, mais l'autorisation a expiré**.
   - Le message d'erreur brut, en rouge, quand l'appel lui-même a échoué.

   À l'écran : la ligne verte Opérationnel affichée dans l'onglet Aperçu de la fiche du serveur.

## Messages d'erreur traduits, à l'ajout d'une connexion

Ces messages appartiennent à la fenêtre qui ajoute une connexion depuis le catalogue, pas au test d'un serveur déjà installé. Sur la fiche du serveur ci-dessus, un test en échec affiche le message du backend tel quel, sans traduction et sans lien **Voir le détail technique**. Quand une connexion est ajoutée et que son essai échoue, Apollia traduit les erreurs techniques en messages clairs :

- *« Authentification refusée - vérifiez votre clé API. »* : token invalide ou expiré (HTTP 401).
- *« Accès interdit - votre clé n'a pas les droits requis. »* : permissions insuffisantes (HTTP 403).
- *« Service introuvable - vérifiez l'URL ou le nom du serveur. »* : mauvaise URL ou serveur absent (HTTP 404).
- *« Erreur réseau - le service n'a pas répondu à temps. »* : timeout ou connexion impossible.
- *« Commande introuvable - le paquet n'est probablement pas installé. »* : transport stdio, binaire absent.
- *« La connexion a échoué. Vérifiez vos identifiants et réessayez. »* : erreur générique.

Dans cette fenêtre, un dépliant **Voir le détail technique** révèle le message brut du backend, **Réessayer** essaie de nouveau la connexion et **Retirer** l'enlève.

## Vérification

- La ligne de résultat dit **Opérationnel** ou **Joignable**, et son nombre d'outils est non nul.
- Les outils attendus apparaissent dans l'onglet **Outils**.
- S'il en manque, regardez d'abord du côté d'Apollia : la découverte garde au plus `max_tools` outils par serveur, 256 par défaut, et journalise `mcp.tools.bounded` avec ce qu'elle a gardé et ce qu'elle a reçu quand elle coupe. La réponse d'un serveur est aussi plafonnée par `max_response_bytes`. Ce n'est qu'une fois ces deux bornes écartées que le serveur distant devient l'explication.

## Si ça ne marche pas

- **Erreur réseau** : votre machine ne peut pas joindre le serveur. Vérifiez votre connexion internet ou, pour un MCP stdio, le PATH de la commande.
- **Authentification refusée** : utilisez **Reconnecter** dans l'en-tête de la fiche, et si cela ne suffit pas, utilisez **Déconnecter** dans l'onglet **Paramètres** puis ajoutez de nouveau le serveur avec des identifiants valides.
- **Accès interdit** : votre compte n'a pas les droits côté fournisseur. Vérifiez les scopes accordés ou augmentez les permissions côté outil métier.
- **Commande introuvable (stdio)** : pour un MCP en transport stdio, le binaire n'est pas dans le PATH d'Apollia. Installez l'outil ou ajustez la commande.
- **Test OK mais l'agent n'appelle pas l'outil** : ouvrez la fiche de l'agent et vérifiez, sous **Accès**, que les outils de cette connexion lui sont accordés. Voir [Comprendre la portée d'une intégration](understand-integration-scope.md).

> **Référence technique :** [Référence Apollia](/reference) , codes d'erreur complets, sémantique handshake MCP.
