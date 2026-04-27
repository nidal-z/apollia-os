# Installer un agent

> Pour tout operator qui veut ajouter un nouvel agent à Apollia : parcourir le catalogue communautaire, vérifier les prérequis, et installer un agent prêt à l'emploi.

## Prérequis

- Un fournisseur d'IA est connecté (pastille verte dans le bandeau supérieur).
- Connexion internet active pour parcourir le catalogue communautaire.
- Vous savez quelle tâche vous voulez confier à l'agent.

## Étapes

1. Dans la sidebar, cliquez sur **Intégrations**.

2. Cliquez sur le bouton **Catalogue** pour ouvrir l'overlay du catalogue communautaire.
   `[SCREENSHOT: page Intégrations, bouton Catalogue surligné, overlay plein écran avec grille de cartes agents]`

3. Filtrez par catégorie (productivité, veille, développement, communication…) ou tapez un mot-clé dans la barre de recherche.

4. Cliquez sur la carte de l'agent qui vous intéresse. Une fiche détaillée s'ouvre avec sa description, ses outils requis, son auteur et son niveau de confiance.

5. Vérifiez la section **Outils requis**. Si l'agent demande des intégrations non installées (Notion, GitHub…), Apollia les signale en orange. Installez-les d'abord depuis la page **Intégrations**.
   `[SCREENSHOT: fiche agent, section "Outils requis" avec deux pastilles vertes et une orange "Notion non installé"]`

6. Cliquez sur **Installer**. L'agent est téléchargé localement en quelques secondes. Un bandeau **Installé localement** apparaît.

7. Allez dans la sidebar, cliquez sur **Mes assistants**. Votre nouvel agent figure dans la liste, classé selon son type :
   - **Assistants** — agent conversationnel avec lequel vous discutez directement.
   - **Agents workers** — agent spécialisé appelé par d'autres (triggers, pipelines).
   `[SCREENSHOT: page Mes assistants avec sections "Assistants" et "Agents workers", nouvelle carte agent visible]`

8. (Optionnel) Cliquez sur **Logs** sur la carte du nouvel agent pour vérifier qu'il a démarré sans erreur.

## Vérification

L'agent apparaît dans **Mes assistants** avec un statut **ARRÊTÉ** (gris) ou **ACTIF** (vert). Sa fiche est consultable et son auteur, sa version et son niveau de confiance sont affichés.

## Si ça ne marche pas

- **Bouton Installer grisé :** vérifiez votre connexion internet et que le bandeau supérieur ne signale pas d'erreur.
- **Outils requis manquants :** rendez-vous dans **Intégrations** pour installer les serveurs MCP demandés avant de réessayer.
- **L'agent disparaît après installation :** consultez ses logs depuis la fiche pour identifier une erreur de démarrage.

> **Référence technique :** [Community-Agent-Registry](https://github.com/nidal-z/apollia-os/wiki/Community-Agent-Registry) — catalogue complet, critères de confiance, soumission d'agents communautaires.
