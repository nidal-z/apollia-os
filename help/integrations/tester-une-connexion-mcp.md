# Tester une connexion MCP

> Pour les operators qui veulent vérifier qu'un serveur MCP déjà connecté répond correctement et expose bien les outils attendus.

## Prérequis

- Un serveur MCP déjà connecté (via le wizard, voyez la page *Connecter un serveur MCP*).
- L'outil métier distant est joignable (compte actif, token non expiré).

## Étapes

1. Dans la sidebar, cliquez sur **Intégrations**.

2. Allez sur l'onglet **Connectés**. Vous voyez la liste de tous vos MCP installés avec leur statut actuel.
   `[SCREENSHOT: page Intégrations, onglet Connectés, liste de cartes MCP avec voyant de statut à gauche]`

3. Cliquez sur la carte du MCP à tester. Le panneau de détail s'ouvre à droite.

4. Cliquez sur le bouton **Tester la connexion** en haut du panneau. Un spinner *Test en cours…* apparaît.
   `[SCREENSHOT: panneau de détail MCP, bouton "Tester la connexion" surligné, zone de résultat en dessous]`

5. Attendez le résultat (entre 1 et 5 secondes en général).

6. Si le test réussit, un voyant vert s'affiche avec un message du type *Connecté — latence : 145 ms* et la liste des outils exposés par le serveur.

7. Si le test échoue, un voyant rouge s'affiche avec un message d'erreur précis (*Token invalide*, *Serveur injoignable*, *Timeout*…). Notez le message exact.
   `[SCREENSHOT: panneau résultat, voyant rouge avec message "Token invalide — vérifiez vos identifiants", bouton "Reconfigurer" en dessous]`

8. Vérifiez que les **outils attendus** apparaissent bien dans la liste. Si certains manquent, le serveur distant a peut-être désactivé des fonctionnalités côté compte.

9. Pour tester un outil concret, ouvrez un chat avec un agent autorisé à utiliser ce MCP et demandez en langage naturel une action simple (par exemple : *Liste mes 3 dernières pages Notion*).

10. Observez la réponse : si l'outil est appelé et renvoie un résultat cohérent, l'intégration est pleinement fonctionnelle.

## Vérification

Le voyant est vert, la latence est inférieure à 1 seconde, tous les outils attendus sont listés, et un agent réussit à appeler l'un d'eux depuis un chat.

## Si ça ne marche pas

- **Voyant rouge — Token invalide** : régénérez le token dans l'outil métier, puis cliquez sur **Reconfigurer** pour mettre à jour vos identifiants.
- **Voyant rouge — Serveur injoignable** : vérifiez votre connexion internet et l'URL du transport (HTTP/SSE). Pour un transport stdio, vérifiez que la commande locale est encore installée.
- **Test OK mais l'agent n'appelle pas l'outil** : ouvrez la fiche de l'agent et vérifiez que ce MCP figure bien dans ses outils autorisés.

> **Référence technique :** [Briques-MCP](https://github.com/nidal-z/apollia-os/wiki/Briques-MCP)
