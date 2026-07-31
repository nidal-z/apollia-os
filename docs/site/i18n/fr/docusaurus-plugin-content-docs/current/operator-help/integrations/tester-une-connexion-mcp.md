# Tester une connexion MCP

> Pour tout operator qui veut vérifier qu'un serveur MCP installé répond bien, ou diagnostiquer un voyant rouge.

## Prérequis

- Au moins un serveur MCP installé (voir [Connecter un serveur MCP](connecter-un-serveur-mcp.md)).

## Étapes

1. Dans la sidebar **Connexions**, sélectionnez le serveur MCP à tester.

   ![Page Connexions : un serveur MCP sélectionné dans la barre latérale, sa fiche de détail à droite](/img/operator-help/integration-tester-une-connexion-mcp-1.png)

2. Dans le panneau de détail, cliquez sur l'icône **plug** (prise) à côté du nom du serveur, ou sur **Tester la connexion** dans le menu d'actions.

   ![Fiche d'un serveur MCP installé, avec le bouton Test dans la zone d'actions](/img/operator-help/integration-tester-une-connexion-mcp-2.png)

3. Pendant le test, l'icône pulse et le bouton est désactivé. Le test dure typiquement moins d'une seconde.

4. Le résultat s'affiche sous forme d'un badge :

   - **Vert** : *"OK · XXX ms"*. Le serveur répond, la latence est indiquée.
   - **Rouge** : *"Erreur : <message traduit>"*. Le serveur ne répond pas, le message précise la cause.

   A l'ecran : le badge vert OK · 247 ms affiché sous le bouton de test.

## Messages d'erreur traduits

Apollia traduit les erreurs techniques en messages clairs :

- *"Authentification refusée, vérifiez votre clé API"* : token invalide ou expiré (HTTP 401).
- *"Accès interdit, votre clé n'a pas les droits requis"* : permissions insuffisantes (HTTP 403).
- *"Service introuvable, vérifiez l'URL ou le nom du serveur"* : mauvaise URL ou serveur absent (HTTP 404).
- *"Erreur réseau, le service n'a pas répondu à temps"* : timeout ou connexion impossible.
- *"Commande introuvable, le paquet n'est probablement pas installé"* : transport stdio, binaire absent.
- *"La connexion a échoué, vérifiez vos identifiants et réessayez"* : erreur générique.

Un lien **Voir le détail technique** affiche le message brut du backend pour les utilisateurs avancés.

## Vérification

- Latence inférieure à 1 seconde pour un serveur en bonne santé.
- Le compteur d'outils dans le panneau de détail est non nul.
- Les outils attendus apparaissent dans la liste. Si certains manquent, le serveur distant a peut-être désactivé des fonctionnalités côté compte.

## Si ça ne marche pas

- **Erreur réseau** : votre machine ne peut pas joindre le serveur. Vérifiez votre connexion internet ou, pour un MCP stdio, le PATH de la commande.
- **Authentification refusée** : déconnectez le MCP puis reconnectez avec des identifiants valides, ou utilisez le bouton **Modifier l'authentification** pour mettre à jour le token sans tout réinstaller.
- **Accès interdit** : votre compte n'a pas les droits côté fournisseur. Vérifiez les scopes accordés ou augmentez les permissions côté outil métier.
- **Commande introuvable (stdio)** : pour un MCP en transport stdio, le binaire n'est pas dans le PATH d'Apollia. Installez l'outil ou ajustez la commande.
- **Test OK mais l'agent n'appelle pas l'outil** : ouvrez la fiche de l'agent, vérifiez que ce MCP figure dans son manifest. Voir [Comprendre la portée d'une intégration](comprendre-la-portee-d-une-integration.md).

> **Référence technique :** [Référence Apollia](/reference) , codes d'erreur complets, sémantique handshake MCP.
