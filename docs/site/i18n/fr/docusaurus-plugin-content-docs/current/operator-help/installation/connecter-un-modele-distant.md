---
title: Connecter un modèle distant
slug: /operator-help/installation/connect-a-remote-model
sidebar_position: 6
---

# Connecter un modèle distant

> Pour toute personne qui veut brancher à Apollia le serveur d'IA de son organisation, ou un service qu'elle utilise déjà : Ollama, vLLM, LM Studio, une passerelle LiteLLM, ou tout serveur doté d'une interface compatible OpenAI.

## Prérequis

- Apollia lancé.
- L'adresse du serveur, telle qu'il la donne à ses clients, avec son port.
- Une clé, seulement si le serveur en demande une.
- Un chemin réseau entre votre machine et le serveur.

## Ce que ce choix implique

Deux points à connaître avant de brancher un serveur.

**Vos prompts quittent la machine.** Un serveur reçoit ce qu'Apollia lui envoie,
ce qui peut inclure du contenu de fichiers, des souvenirs de la mémoire et des
données de l'espace de travail. C'est le comportement attendu d'un modèle
distant, et c'est précisément ce qu'un modèle local évite : avec le moteur
`llama-server` embarqué, rien ne sort. Le choix vous appartient, il doit juste
être conscient.

**En `http://`, la clé circule en clair.** Si l'adresse ne commence pas par
`https://` et pointe vers une autre machine, la clé transite sans chiffrement sur
le réseau. Sur un réseau local de confiance ou dans un tunnel, c'est acceptable.
Sinon, préférez `https://`. Un serveur qui ne demande pas de clé n'est concerné
que par le premier point.

## Pour quel cas d'usage

Apollia distingue trois cas. Choisissez selon ce que vous voulez faire :

- **Brancher un serveur** : c'est cette page. Une adresse suffit, rien à télécharger.
- **Faire tourner un modèle local**, servi par le moteur embarqué et géré automatiquement par Apollia : voir [Télécharger des modèles locaux](telecharger-des-modeles-locaux.md). C'est le choix qui garde tout sur la machine.
- **Voir et gérer les serveurs déjà enregistrés** : ouvrez l'écran **IA**, sous **Serveurs**.

## Étapes

1. Dans la barre latérale, ouvrez **IA**.
2. Sous **Serveurs**, cliquez sur **Ajouter un serveur**. Le formulaire s'ouvre dans l'écran.
3. Saisissez l'**Adresse** du serveur, avec `http://` ou `https://` et son port, par exemple `http://localhost:11434`. Elle se termine souvent par `/v1`.
4. Vérifiez le **Nom**, qu'Apollia propose à partir de l'adresse : lettres minuscules, chiffres et tirets. C'est le nom sous lequel le serveur apparaît dans Apollia, et il ne change plus une fois le serveur enregistré.
5. Renseignez la **Clé** seulement si le serveur en demande une. Apollia la garde avec ses secrets et ne l'affiche plus jamais.
6. Cliquez sur **Tester**. Apollia demande au serveur la liste de ses modèles et n'enregistre rien : un message confirme la connexion et le nombre de modèles trouvés.
7. Cliquez sur **Enregistrer**. Le serveur apparaît sous **Serveurs**, avec son état et ses modèles. Le test n'est pas obligatoire : un serveur enregistré alors qu'il est éteint montre ses modèles dès qu'il répond.
8. (Optionnel) Dans la liste des modèles du serveur, ouvrez le menu d'un modèle et choisissez **Utiliser par défaut** pour que les conversations et les agents répondent avec lui quand aucun autre modèle n'est choisi.

## Ce que montre un serveur enregistré

- **Son état** : **Actif** quand il répond et que ses modèles sont proposés, **Désactivé** quand vous l'avez coupé avec l'interrupteur **Actif**, **Injoignable** quand rien ne répond à son adresse, **En erreur** quand quelque chose répond mais pas avec une liste de modèles.
- **Tester** redemande ses modèles au serveur, et **Modifier** rouvre le formulaire pour changer l'adresse ou la clé.
- **Ses modèles**, chacun avec la **Fenêtre de contexte** avec laquelle le serveur le sert. Un modèle marqué **Fenêtre courte** est servi avec une fenêtre plus courte que celle dont un agent a besoin : une longue conversation ou une tâche à plusieurs étapes peut être coupée, et seule la personne qui gère le serveur peut l'agrandir.
- Le menu du serveur copie son adresse ou le supprime. Supprimer un serveur oublie sa clé.

Apollia ne modifie pas les réglages de chargement des modèles d'un serveur : ils appartiennent à la personne qui le gère.

## Adresses des serveurs courants

- **Ollama** : `http://<host>:11434`, ou `http://localhost:11434` quand Ollama tourne sur votre machine. Le service Ollama doit être démarré sur cet hôte.
- **vLLM, LM Studio, LiteLLM et équivalents** : l'adresse que leur documentation donne pour l'interface compatible OpenAI, qui se termine en général par `/v1`.
- **Un service en ligne doté d'une interface compatible OpenAI** : l'adresse que donne sa documentation, avec la clé qu'il vous a délivrée.

Pour un modèle géré directement par Apollia via son moteur embarqué, sans serveur, voir [Télécharger des modèles locaux](telecharger-des-modeles-locaux.md).

## Routage hybride : escalader vers un modèle de pointe

Le routage hybride permet à Apollia d'utiliser un modèle local par défaut et de basculer automatiquement vers un modèle distant plus capable pour les étapes qui dépassent les capacités locales, dans la limite d'un plafond de coût. Il se règle dans le fichier de configuration, pas dans l'application : voir [Configurer le routage hybride local + frontier](configure-hybrid-routing.md).

## Vérification

- Le serveur apparaît sous **Serveurs** avec l'état **Actif**, et ses modèles sont listés.
- Ouvrez un chat, choisissez l'un de ses modèles dans le sélecteur d'agents et de modèles, sous **Modèles**, et envoyez un message court. La réponse arrive au fil de l'eau.

## Si ça ne marche pas

- **Le serveur ne répond pas** : vérifiez que le serveur est démarré, que l'adresse et le port sont justes, et que ce poste peut le joindre sur le réseau. Pour Ollama, `curl http://<host>:11434/api/tags` depuis votre machine dit s'il est joignable.
- **Le serveur refuse la clé** : la clé est absente, mal saisie ou n'est plus valide. Recopiez-la sans espaces parasites.
- **Cette adresse ne répond pas comme un serveur d'IA** : quelque chose répond, mais sans liste de modèles. Utilisez l'adresse de l'interface du serveur, pas celle de sa page d'accueil.
- **Le serveur ne propose aucun modèle** : il répond, mais sa liste est vide. Installez ou chargez un modèle sur le serveur, puis testez de nouveau.
- **La politique de votre organisation refuse ce serveur** : utilisez un serveur que votre organisation autorise, ou demandez à la personne qui gère Apollia pour vous.
- **Pas de réponse dans le chat alors que le serveur est Actif** : voir [Le fournisseur d'IA ne répond pas](../troubleshooting/the-ai-provider-does-not-answer.md).

> **Référence technique :** [Configuration](/reference/configuration) pour la section `[llm]`, ses backends et ses clés de routage, et [Valeurs par défaut d'échantillonnage](/reference/sampling-defaults) pour le seul paramètre d'échantillonnage qu'une requête porte réellement jusqu'au modèle.
