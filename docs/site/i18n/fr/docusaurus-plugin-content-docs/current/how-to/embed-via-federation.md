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
déployer un serveur MCP devant vos données, et que votre produit reçoit les
écritures en retour par ce serveur ou par une API HTTP publique.

## Le principe

Dans le modèle de fédération, les deux systèmes restent pairs :

- **Apollia s'exécute comme un sidecar souverain.** Il assure le travail de
  l'agent : raisonnement, planification, appels d'outils, le tout sous sa
  propre gouvernance (permissions, audit, budgets).
- **Votre produit expose ses données via un serveur MCP.** Apollia s'y
  connecte en tant que client MCP et appelle vos outils pour lire ce dont il a
  besoin. Vos données restent de votre côté ; Apollia les lit à travers les
  outils que vous choisissez d'exposer.
- **Apollia écrit en retour via votre produit.** Quand l'agent a un résultat
  à persister, il appelle un outil d'écriture de votre serveur MCP, ou les
  points de terminaison REST de votre produit lorsqu'ils sont publics, de sorte
  que votre produit reste le système de référence et garde le contrôle de
  chaque écriture.

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
de la forme `mcp:<serveur>/<outil>`). Un outil n'est proposé à un agent que si
sa liste d'accès l'accorde, sur la fiche de l'agent ou dans son paquet. Ces
appels passent par le même chemin gouverné que les outils natifs : ils sont donc
soumis aux permissions et consignés dans le journal d'audit.

Pour le détail de la configuration côté desktop, consultez l'aide opérateur
sur
[la connexion d'un serveur MCP](/operator-help/integrations/connect-an-mcp-server)
et [le câblage de votre propre serveur MCP](/operator-help/integrations/wire-your-own-mcp-server).

## Étape 3 : soumettre les écritures à une approbation humaine

La fédération signifie en général que l'agent peut déclencher des changements
dans votre produit. Gardez un humain dans la boucle sur ces changements.

Un serveur MCP que vous enregistrez ne porte aucun réglage d'approbation qui lui
soit propre. Chaque outil qu'il publie se décide sur sa famille d'action : un
outil que votre serveur déclare comme écrivant ou destructeur, ou qu'il ne décrit
pas du tout, est traité comme sensible, et l'agent qui l'appelle demande d'abord.

```sh
apollia-os mcp add my-product --url https://example.internal/mcp
```

La personne qui suit l'exécution répond une fois, toujours pour cet agent, ou
refuse. « Toujours » est un consentement permanent de cet agent pour cette
famille d'actions ; il tient jusqu'à son retrait. Déclarez honnêtement
`readOnlyHint` et `destructiveHint` sur vos outils : ce sont eux qui distinguent
ici une lecture d'une écriture.

Pour comprendre comment les approbations et les paliers d'autonomie
s'articulent ici, voir [Paliers d'autonomie](/explanation/autonomy-tiers) et
l'explication du [modèle de responsabilité](/explanation/accountability-model).

## Étape 4 : laisser Apollia écrire en retour via votre produit

L'agent a deux façons de persister un résultat dans votre produit, et elles
n'atteignent pas les mêmes endroits.

La première est un outil d'écriture de votre serveur MCP, celui de l'étape 1.
L'appel suit le chemin gouverné de l'étape 3, et votre serveur atteint votre
API où qu'elle tourne, y compris dans votre réseau interne, avec l'identifiant
qu'il lui faut : rien de cet identifiant ne passe par l'agent. C'est la voie à
choisir quand votre API est interne ou demande un secret.

La seconde est `http_fetch`, l'outil natif qui appelle une adresse web. Il
n'est proposé qu'à un agent dont la fiche ou le paquet l'accorde, et sa famille
est `send_external` : un appel est demandé, sauf si un « toujours » de la
personne ou un accord de l'exécution le couvre, et une exécution sans personne
devant attend la réponse. Il refuse toute destination qui n'est pas publique :
adresses de bouclage, privées et de lien local, `localhost`, noms finissant par
`.local`, `.internal` ou `.localdomain`, et une redirection vers l'une d'elles. Le modèle
écrit sa méthode, ses en-têtes et son corps, donc une clé d'API qu'il envoie
doit figurer dans le contexte de l'agent, et il lit au plus 1 Mo de la réponse.
Réservez-le à un point de terminaison public qui ne demande aucun secret.

Dans les deux cas, votre produit valide et stocke le changement, restant ainsi
le système de référence. Si vous pilotez aussi Apollia depuis votre produit
(soumission de tâches, streaming des résultats), ce volet utilise le même
contrat stable décrit dans
[Intégrer Apollia via le contrat de pilotage](/how-to/integrate-via-driving-contract).

## Pourquoi la fédération

Cela maintient la souveraineté de votre côté de la ligne. Vos données sont
lues via les outils que vous exposez et écrites via un serveur ou une API que
vous possédez, tandis qu'Apollia apporte le runtime agentique avec sa gouvernance.
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
