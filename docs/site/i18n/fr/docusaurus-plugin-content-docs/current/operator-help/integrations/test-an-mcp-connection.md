---
title: Tester une connexion MCP
slug: /operator-help/integrations/test-an-mcp-connection
sidebar_position: 6
---

# Tester une connexion MCP

> Pour tout operator qui veut vérifier qu'un serveur MCP installé répond bien, ou diagnostiquer un voyant rouge.

## Prérequis

- Au moins un serveur MCP installé (voir [Connecter un serveur MCP](connecter-un-serveur-mcp.md)).

## Étapes

1. Dans la sidebar **Connexions**, sélectionnez le serveur MCP à tester.

   ![Page Connexions : un serveur MCP sélectionné dans la barre latérale, sa fiche de détail à droite](/img/operator-help/integration-tester-une-connexion-mcp-1.png)

2. Dans l'en-tête du panneau de détail, cliquez sur **Tester**. C'est un bouton simple à côté de **Reconnecter**, avec une icône de rafraîchissement ; il n'y a ni icône de prise ni menu d'actions.

   ![Fiche d'un serveur MCP installé, avec le bouton Test dans la zone d'actions](/img/operator-help/integration-tester-une-connexion-mcp-2.png)

3. Pendant le test, le bouton affiche un indicateur d'activité et se désactive. Le test dure typiquement moins d'une seconde.

4. Le résultat s'affiche sur une ligne sous les onglets. Elle porte un nombre d'outils, jamais une latence : rien sur le chemin MCP ne mesure un temps de réponse.

   - **Opérationnel - N outils, dernière opération réussie**, en vert.
   - **Joignable - N outils listés, pas encore vérifié par une opération**, en gris. Le serveur a répondu à la poignée de main, mais aucune opération ne l'a confirmé depuis.
   - **Joignable, mais des opérations récentes ont échoué** ou **Joignable, mais l'autorisation a expiré**, en orange.
   - Le message d'erreur brut, en rouge, quand l'appel lui-même a échoué.

   A l'ecran : la ligne verte Opérationnel affichée sous les onglets de la fiche du serveur.

## Messages d'erreur traduits, dans l'assistant d'installation

Ces messages appartiennent à l'assistant qui installe un serveur, pas au test d'un serveur déjà installé. Sur le panneau de détail ci-dessus, un test en échec affiche le message du backend tel quel, sans traduction et sans lien **Voir le détail technique**. Dans l'assistant, Apollia traduit les erreurs techniques en messages clairs :

- *"Authentification refusée, vérifiez votre clé API"* : token invalide ou expiré (HTTP 401).
- *"Accès interdit, votre clé n'a pas les droits requis"* : permissions insuffisantes (HTTP 403).
- *"Service introuvable, vérifiez l'URL ou le nom du serveur"* : mauvaise URL ou serveur absent (HTTP 404).
- *"Erreur réseau, le service n'a pas répondu à temps"* : timeout ou connexion impossible.
- *"Commande introuvable, le paquet n'est probablement pas installé"* : transport stdio, binaire absent.
- *"La connexion a échoué, vérifiez vos identifiants et réessayez"* : erreur générique.

Dans l'assistant, un dépliant **Voir le détail technique** révèle le message brut du backend, et un bouton **Modifier l'authentification** vous renvoie à l'étape des identifiants.

## Vérification

- La ligne de résultat dit **Opérationnel** ou **Joignable**, et son nombre d'outils est non nul.
- Les outils attendus apparaissent dans l'onglet **Outils**.
- S'il en manque, regardez d'abord du côté d'Apollia : la découverte garde au plus `max_tools` outils par serveur, 256 par défaut, et journalise `mcp.tools.bounded` avec ce qu'elle a gardé et ce qu'elle a reçu quand elle coupe. La réponse d'un serveur est aussi plafonnée par `max_response_bytes`. Ce n'est qu'une fois ces deux bornes écartées que le serveur distant devient l'explication.

## Si ça ne marche pas

- **Erreur réseau** : votre machine ne peut pas joindre le serveur. Vérifiez votre connexion internet ou, pour un MCP stdio, le PATH de la commande.
- **Authentification refusée** : utilisez **Reconnecter** dans l'en-tête de la fiche, et si cela ne suffit pas, déconnectez le serveur puis réinstallez-le avec des identifiants valides.
- **Accès interdit** : votre compte n'a pas les droits côté fournisseur. Vérifiez les scopes accordés ou augmentez les permissions côté outil métier.
- **Commande introuvable (stdio)** : pour un MCP en transport stdio, le binaire n'est pas dans le PATH d'Apollia. Installez l'outil ou ajustez la commande.
- **Test OK mais l'agent n'appelle pas l'outil** : ouvrez la fiche de l'agent, vérifiez que ce MCP figure dans son manifest. Voir [Comprendre la portée d'une intégration](understand-integration-scope.md).

> **Référence technique :** [Référence Apollia](/reference) , codes d'erreur complets, sémantique handshake MCP.
