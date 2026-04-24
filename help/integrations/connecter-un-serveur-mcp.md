# Connecter un serveur MCP

> Pour les operators qui veulent brancher un outil métier (Notion, GitHub, Slack, base de données…) sur leurs agents, sans écrire de code.

## Prérequis

- Vous savez quel outil métier vous voulez brancher.
- Vous avez les **identifiants** ou le **token** d'accès nécessaires à cet outil.
- Connexion internet active (pour parcourir le catalogue).

## Étapes

1. Dans la sidebar, cliquez sur **Intégrations**.

2. Restez sur l'onglet **Catalogue**. Filtrez par catégorie (productivité, développement, communication…) ou tapez le nom de l'outil cherché dans la barre de recherche.
   `[SCREENSHOT: page Intégrations, onglet Catalogue, filtres latéraux par catégorie et niveau de confiance, recherche "Notion" tapée]`

3. Cliquez sur la carte du serveur souhaité. Vous voyez sa description, son auteur, son niveau de confiance (officiel, vérifié, communautaire), et la liste des outils qu'il expose.

4. Cliquez sur **Installer**. Apollia télécharge et prépare le serveur (quelques secondes), puis le **wizard de configuration** s'ouvre automatiquement.

5. **Étape 1 — Type de transport** :
   - **stdio** (recommandé) — le serveur tourne comme processus local, isolé, le plus sécurisé.
   - **HTTP** — le serveur tourne ailleurs, accessible via une URL.
   - **SSE** — pour les serveurs qui poussent des événements en streaming.
   `[SCREENSHOT: ConnectorWizard étape 1, trois cartes de transport stdio/HTTP/SSE, carte stdio sélectionnée]`

6. **Étape 2 — Identifiants**. Renseignez les paramètres demandés (token, URL, clé). Apollia les chiffre localement et ne les transmet à aucun tiers.

7. **Étape 3 — Paramètres complémentaires**. Selon le serveur, vous pouvez préciser un identifiant de base, un dépôt cible, ou un espace de travail. Cette étape est facultative pour la plupart des MCP.

8. **Étape 4 — Tester la connexion**. Cliquez sur **Tester**. Un voyant vert apparaît avec la liste des outils détectés. Si le voyant est rouge, le message d'erreur indique précisément ce qui manque.
   `[SCREENSHOT: étape Test du wizard, voyant vert "Connecté", latence affichée, liste des outils détectés en dessous]`

9. **Étape 5 — Confirmation**. Lisez le disclaimer (responsabilité du serveur communautaire) et cliquez sur **Activer**. Le connecteur passe en statut **Actif** dans la liste de vos intégrations.

10. Pour utiliser le MCP depuis un chat, ouvrez une conversation avec un agent qui a la permission d'utiliser ses outils, et formulez votre demande en langage naturel (par exemple : *Liste mes pages Notion récentes*). L'agent appelle automatiquement les bons outils.

## Vérification

Dans l'onglet **Connectés**, votre MCP apparaît avec un voyant vert et la liste de ses outils disponibles. Un agent autorisé peut désormais l'invoquer depuis n'importe quel chat ou pipeline.

## Si ça ne marche pas

- **Voyant rouge à l'étape Test** : le token est invalide ou expiré. Régénérez-le côté outil métier puis revenez à l'étape 2 du wizard.
- **Aucun outil détecté** : le serveur tourne mais ne déclare rien. Vérifiez la version installée dans la fiche du catalogue ou consultez la page *Tester une connexion MCP*.
- **L'agent dit qu'il n'a pas accès à l'outil** : ouvrez la fiche de l'agent et vérifiez que le MCP figure bien dans ses outils autorisés.

> **Concept :** [book ch04 — Les outils](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch04-00-les-outils.md)
