# Connecter un serveur MCP depuis le catalogue

> Pour tout operator qui veut activer un serveur MCP du catalogue (Notion, GitHub, Linear, Atlassian, Stripe, Time, etc.) en quelques clics.

## Prérequis

- Apollia lancé, page **Connexions** accessible.
- Vous savez quel service vous voulez brancher. Le catalogue v0.1.0 propose 18 entrées soigneusement sélectionnées (voir la liste complète dans [Vue d'ensemble des intégrations](vue-d-ensemble-integrations.md)).
- Pour les services authentifiés, vos identifiants (clé API ou compte OAuth chez le fournisseur).

## Étapes

1. Dans la sidebar, ouvrez **Connexions**, puis cliquez sur **+ Découvrir** en haut. Le catalogue s'ouvre en panneau dédié.

   `[SCREENSHOT: page Connexions, bouton "+ Découvrir" en haut à droite, panneau catalogue ouvert avec onglet Découvrir actif et grille d'entrées]`

2. Filtrez ou cherchez l'entrée souhaitée, puis cliquez dessus. L'assistant en 4 étapes démarre.

### Étape 1, Disclaimer

Quatre cases à cocher rappellent les implications d'installer un MCP externe (du code tiers s'exécute sur votre machine, des données peuvent être transférées, vous pouvez révoquer à tout moment, les capabilities sont visibles avant install). Cochez les quatre, puis cliquez **Suivant**.

`[SCREENSHOT: étape 1 du wizard, 4 cases à cocher avec leurs libellés, bouton Suivant grisé tant que tout n'est pas coché]`

### Étape 2, Authentification

Apollia détecte automatiquement le type d'authentification requis par le serveur. Trois cas possibles :

- **Aucune authentification** : message *"Pas d'authentification nécessaire"*. Cliquez **Suivant**.
- **Clé API ou jeton statique** : un champ mot de passe apparaît. Collez votre clé.
- **OAuth** : un bouton *"Se connecter avec [Provider]"* apparaît avec la liste des scopes demandés. Cliquez, votre navigateur ouvre la page de consentement, autorisez, le retour est automatique.

`[SCREENSHOT: étape 2 du wizard exemple OAuth, bouton "Se connecter avec [Provider]" et liste des scopes en dessous]`

### Étape 3, Test

Cliquez sur **Tester la connexion**. Pendant le test, l'icône pulse. À la fin, un badge affiche le résultat :

- **Vert** : *"X outils détectés"*. Le serveur répond.
- **Rouge** : message d'erreur précis (clé invalide, URL injoignable, etc.).

Si le test échoue, revenez à l'étape 2 pour corriger.

`[SCREENSHOT: étape 3 du wizard, bouton "Tester la connexion" et badge vert "12 outils détectés"]`

### Étape 4, Coaching

Apollia affiche quelques cartes d'exemples avec un bouton *"Essayer ce prompt"* qui pré-remplit la zone de chat. Cliquez **Terminer** pour clôturer l'assistant.

`[SCREENSHOT: étape 4 du wizard, 3 cartes d'exemples avec bouton "Essayer ce prompt" et bouton Terminer]`

## Vérification

- Le serveur apparaît dans la sidebar **Connexions** avec une pastille verte.
- Le panneau de détail affiche les outils déclarés par le serveur, avec leur description.
- Dans le chat libre, lancez un prompt suggéré par l'étape Coaching. L'outil correspondant est appelé.

> **Note - chargement différé :** par défaut, `[mcp] tool_loading = "deferred"`. Les outils du serveur ne sont pas tous chargés en contexte au démarrage : l'agent invoque `tool_search` à la demande pour récupérer l'outil pertinent. Le nombre d'outils affiché dans l'UI reste complet. Ce comportement est intentionnel et permet de gérer des serveurs avec de nombreux outils sans saturer le contexte.

## Si ça ne marche pas

- **Le test échoue avec "Authentification refusée"** : votre clé ou token est invalide ou révoqué. Revenez à l'étape 2 et recollez la valeur sans espaces parasites.
- **Le test échoue avec "Service introuvable"** : le serveur n'est pas joignable. Vérifiez votre connexion ou le statut du fournisseur.
- **Le serveur installé n'expose aucun outil** : le serveur démarre mais ne déclare rien. Voir [Tester une connexion MCP](tester-une-connexion-mcp.md) pour relancer le test, puis vérifier les logs côté fournisseur.
- **Vous voulez brancher un serveur qui n'est pas dans le catalogue** : voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
- **L'agent dit qu'il n'a pas accès à l'outil en mode deferred** : en mode `deferred`, l'agent doit appeler `tool_search` pour charger l'outil à la demande. Si l'agent ne le fait pas, vérifiez que son manifest liste bien ce serveur MCP parmi ses connexions autorisées. Sinon, mettez le manifest à jour.
- **L'agent dit qu'il n'a pas accès à l'outil** : ouvrez la fiche de l'agent, l'onglet Outils liste ce que son manifest déclare. Si l'outil n'y figure pas, c'est l'agent qu'il faut mettre à jour. Voir [Comprendre la portée d'une intégration](comprendre-la-portee-d-une-integration.md).

> **Référence technique :** [Référence Apollia](../../reference/index.md) , protocole MCP, transports, trust levels, gouvernance.
