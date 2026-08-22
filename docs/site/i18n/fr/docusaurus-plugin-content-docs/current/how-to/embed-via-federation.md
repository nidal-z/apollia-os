---
sidebar_position: 2
title: Intégrer Apollia par fédération (MCP + REST)
---

# Intégrer Apollia par fédération (MCP + REST)

Ce guide explique comment intégrer Apollia dans un produit hôte en tant que
sidecar souverain, sans faire transiter vos données dans le runtime. Il
convient aux produits dont les données ne peuvent pas franchir leur périmètre
de confiance, mais qui veulent malgré tout des agents autonomes agissant
dessus.

Il suppose que vous pouvez exécuter un daemon Apollia, que vous pouvez
déployer un serveur MCP devant vos données, et que votre produit expose une
API HTTP qu'Apollia peut rappeler.

## Le principe

Dans le modèle de fédération, les deux systèmes restent pairs :

- **Apollia s'exécute comme un sidecar souverain.** Il assure le travail de
  l'agent : raisonnement, planification, appels d'outils, le tout sous sa
  propre gouvernance (permissions, audit, budgets).
- **Votre produit expose ses données via un serveur MCP.** Apollia s'y
  connecte en tant que client MCP et appelle vos outils pour lire ce dont il a
  besoin. Vos données restent de votre côté ; Apollia les lit à travers les
  outils que vous choisissez d'exposer.
- **Apollia écrit en retour via votre API HTTP.** Quand l'agent a un résultat
  à persister, il appelle les points de terminaison REST de votre produit, de
  sorte que votre produit reste le système de référence et garde le contrôle
  de chaque écriture.

Apollia est souvent le client de l'hôte, pas l'inverse. Rien n'est copié dans
le runtime que vous n'ayez délibérément exposé.

## Étape 1 : exposer vos données via MCP

Déployez un serveur MCP qui expose les données et les actions que vous voulez
rendre disponibles à l'agent. Le client MCP d'Apollia parle le protocole
standard (`initialize` puis `tools/list`) sur trois transports : stdio,
streamable HTTP et SSE. Choisissez le transport adapté à la façon dont votre
serveur est déployé par rapport au runtime.

Exposez des outils de lecture pour le contexte dont l'agent a besoin, et
gardez les outils d'écriture étroits et explicites. L'agent ne peut utiliser
que ce que votre serveur annonce.

## Étape 2 : connecter Apollia à votre serveur MCP

Enregistrez votre serveur auprès du runtime et vérifiez que ses outils sont
bien découverts. Une fois connecté, un agent exécuté à l'intérieur d'Apollia
invoque vos outils MCP via son interface d'outils (les noms d'outils sont
préfixés par un espace de noms `mcp:`). Ces appels passent par le même chemin
gouverné que les outils natifs : ils sont donc soumis aux permissions et
consignés dans le journal d'audit.

Pour le détail de la configuration côté desktop, consultez l'aide opérateur
(en français) sur
[la connexion d'un serveur MCP](/operator-help/integrations/connecter-un-serveur-mcp)
et [le câblage de votre propre serveur MCP](/operator-help/integrations/cabler-son-propre-serveur-mcp).

## Étape 3 : soumettre les écritures à une approbation humaine

La fédération signifie en général que l'agent peut déclencher des changements
dans votre produit. Gardez un humain dans la boucle sur ces changements.

Pour un serveur MCP que vous enregistrez, l'approbation se règle par serveur
ou par outil :

```sh
apollia-os mcp add my-product https://example.internal/mcp --require-approval
apollia-os mcp set-approval my-product write_record
apollia-os mcp list-pending
```

Un opérateur confirme alors avant que quoi que ce soit ne soit écrit en
retour. L'autorisation elle-même est persistée, de sorte que l'opérateur n'est
pas resollicité tant qu'elle tient ; la décision n'est écrite dans aucun
registre d'audit, et l'appel qu'elle autorise non plus. Traitez l'approbation
comme une porte, pas comme une preuve.

Une limite à anticiper plutôt qu'à découvrir. Ce flux d'approbation couvre
le chemin du **chat** ; les appels d'outils qu'effectue un agent Python
installé ne sont pas soumis à cette porte, donc ne comptez pas sur les
approbations pour contenir un agent que vous n'avez pas écrit vous-même.
Ce qui fonctionne réellement, c'est l'approbation MCP par serveur décrite
ci-dessus, les règles de préfixe persistées, et le garde-fou qui refuse une
commande shell chaînée.

Pour comprendre comment les approbations et les paliers d'autonomie
s'articulent ici, voir [Paliers d'autonomie](/explanation/autonomy-tiers) et
l'explication du [modèle de responsabilité](/explanation/accountability-model).

## Étape 4 : laisser Apollia écrire en retour via REST

Quand l'agent produit un résultat, il appelle l'API HTTP de votre produit
pour le persister. Votre produit valide et stocke le changement, restant
ainsi le système de référence. Si vous pilotez aussi Apollia depuis votre
produit (soumission de tâches, streaming des résultats), ce volet utilise le
même contrat stable décrit dans
[Intégrer Apollia via le contrat de pilotage](/how-to/integrate-via-driving-contract).

## Pourquoi la fédération

Cela maintient la souveraineté de votre côté de la ligne. Vos données sont
lues via les outils que vous exposez et écrites via une API que vous
possédez, tandis qu'Apollia apporte le runtime agentique avec sa gouvernance.
C'est le modèle d'intégration pour les produits qui ne peuvent pas confier
leurs données à un bac à sable cloud, mais veulent malgré tout des agents
autonomes et auditables.

## Voir aussi

- [Intégrer Apollia via le contrat de pilotage](/how-to/integrate-via-driving-contract)
  pour piloter le runtime depuis votre produit.
- [Auditer et vérifier une exécution](/how-to/audit-and-verify) pour la trace
  que laisse chaque action fédérée.
- [Le modèle de responsabilité](/explanation/accountability-model) pour la
  gouvernance qui soutient ce schéma.
