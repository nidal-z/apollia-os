---
sidebar_position: 2
title: Le modèle de redevabilité
---

# Le modèle de redevabilité

Des agents autonomes ne sont utilisables dans un cadre sérieux que si l'on peut
répondre après coup à deux questions : qu'a fait l'agent, et peut-on faire
confiance à cet enregistrement. Le modèle de redevabilité d'Apollia existe pour
répondre aux deux, et pour garder un humain aux commandes pendant que l'agent
s'exécute. Cette page explique comment les pièces s'assemblent, et ce qu'elles
sont censées apporter, ou non.

## Le problème

Un agent qui raisonne et agit de sa propre initiative est puissant et, sans
contrôles, opaque et irréversible. Dans un cadre réglementé, c'est disqualifiant.
La réponse ne consiste pas à rendre l'agent moins autonome, mais à envelopper
son autonomie dans une gouvernance : borner ce qu'il peut faire, enregistrer
tout ce qu'il fait d'une manière qui ne peut pas être discrètement modifiée, et
garder une personne dans la boucle sur les actions à conséquences. Ces
contrôles sont intégrés au runtime plutôt que laissés à la charge de chaque
agent, de sorte qu'un auteur d'agent ne puisse pas les oublier et qu'un
opérateur ne puisse pas être surpris par leur absence.

## Les briques de base

### Une piste signée, à altération détectable

Chaque action gouvernée qu'un agent effectue est écrite dans un journal
append-only. Le journal est une chaîne de hachage, et il est signé au fur et à
mesure de sa croissance, si bien que toute altération ultérieure brise la
chaîne et devient détectable. Chaque entrée est liée deux fois : à l'entrée
précédente de sa propre exécution, et à l'entrée précédente de n'importe quelle
exécution, si bien que le journal forme une séquence continue unique à travers
toutes les exécutions, et non un ensemble de journaux indépendants par
exécution. C'est ce qui transforme « l'agent dit avoir fait X » en « voici la
séquence enregistrée et vérifiable de ce qui s'est passé ». La piste capture
les appels d'outil et, dans le journal d'exécution, les complétions du modèle
lui-même, si bien que le raisonnement derrière une action est inspectable, pas
seulement son effet.

### La vérification de cette piste

Un enregistrement ne vaut que par la capacité qu'on a à lui faire confiance.
Vérifier le journal contrôle ses chaînes de hachage et ses signatures et
indique si la séquence a été altérée depuis son écriture. Comme les entrées
sont chaînées à travers toutes les exécutions, la vérification détecte non
seulement une entrée modifiée, mais aussi une exécution dont la fin a été
tronquée ou une exécution entière qui a été supprimée : l'un comme l'autre
laisse un trou que la chaîne expose. La redevabilité repose sur un
enregistrement que l'on peut confirmer de façon indépendante, pas sur la
confiance accordée au processus qui l'a produit.

Le périmètre honnête compte ici. Il s'agit de détection d'altération, pas
d'inviolabilité : le mécanisme détecte une altération après coup, il ne
l'empêche pas. La garantie tient tant que la clé de signature n'est pas
compromise. Une partie qui détient la clé peut recalculer et resigner une
chaîne plus courte mais cohérente, c'est pourquoi le runtime expose aussi la
tête de la chaîne comme une ancre que l'on peut exporter et stocker hors de la
machine. Comparer une exécution à une ancre détenue à l'extérieur est ce qui
défend contre la troncature de l'activité la plus récente, même quand la clé
elle-même est compromise.

Pour les commandes derrière ces deux mécanismes, voir
[Auditer et vérifier une exécution](/how-to/audit-and-verify).

L'annulation de ce qu'un agent a écrit est délibérément absente. Un journal
réversible existe dans la base de code, mais rien ne l'installe sur les outils
qui écrivent des fichiers, si bien qu'aucune installation `v0.1.0-preview`
n'enregistre quoi que ce soit à annuler. Livrer la commande quand même aurait
été pire que de ne rien livrer : un résultat vide est indiscernable d'une
session propre, si bien qu'un opérateur lirait « rien à annuler » comme un
filet de sécurité opérationnel et déléguerait en conséquence. Traitez chaque
modification de système de fichiers faite par un agent comme définitive, et
donnez-lui une racine de bac à sable que vous acceptez de perdre.

### Permissions et supervision humaine

<!-- claim:permission-engine-not-wired -->
<!-- claim:hitl-wired-in-chat-path-only -->
Avant qu'un appel d'outil ne s'exécute dans une session de chat, des règles de
permission persistées le classent, un garde-fou refuse une commande shell qui
enchaîne ou redirige, et tout ce qui reste soulève une demande d'approbation
qu'un opérateur résout. Cette décision est elle-même enregistrée. Les
permissions sont scopées, si bien que l'autorité peut être accordée au niveau
de l'installation entière, d'un projet, ou d'une seule session.

Deux limites, à énoncer clairement toutes les deux. `apollia-permissions`
embarque aussi un `PermissionEngine` avec une liste blanche et un détecteur
d'injection shell ; **aucun appelant en production ne l'installe**, si bien que
ces deux composants ne s'exécutent jamais. Et l'enveloppe d'approbation n'est
posée que sur le dispatcher du **chat** : les appels d'outil qu'un agent Python
installé effectue via `ctx.tools` ne rencontrent aucun point de contrôle
humain. C'est une position délibérée, pas un oubli, et le
[modèle de confiance des agents](/explanation/agent-trust-model) explique
pourquoi : un agent installé exécute déjà du Python arbitraire sous votre
compte, si bien qu'une porte sur un seul chemin d'appel ne contiendrait pas un
agent hostile.

### Paliers d'autonomie

Ce qu'un agent peut faire sans demander est fixé par un palier d'autonomie.
Les paliers bas gardent un humain dans la boucle sur davantage d'actions ; les
paliers hauts élargissent ce que l'agent peut faire de sa propre initiative.
Le palier est un curseur délibéré que l'opérateur règle, pas une propriété de
l'agent, si bien que le même agent peut s'exécuter avec prudence ou en toute
liberté selon le contexte et la confiance que l'opérateur lui accorde.

### Garde-fous non négociables

Le runtime impose un budget de pas sur chaque exécution autonome : un
plafond sur le nombre d'étapes de raisonnement, sur le nombre d'appels
d'outil, et sur le temps réel écoulé. Il est imposé par le runtime lui-même et
ne peut pas être contourné par un agent, si bien qu'une exécution ne peut ni
boucler ni consommer sans limite. C'est la garantie que l'autonomie a une
limite dure.

### Filtrage des commandes shell

<!-- claim:injection-detector-is-shell-not-prompt -->
Les commandes shell sont filtrées avant exécution : un classifieur de risque
lit la commande et un contrôle syntaxique rejette ce qui ne peut pas être
analysé. Quand une règle de préfixe permanente est consultée pour un exécuteur
de code, un garde-fou plus strict refuse toute commande qui enchaîne, met en
pipe, redirige ou substitue, si bien qu'une autorisation accordée pour une
commande ne peut pas en faire passer une seconde en contrebande ; en dehors
d'une règle correspondante, chaque invocation d'exécuteur de code exige sa
propre approbation. Le filtrage est enregistré.

Ce mécanisme filtre l'injection **shell**. Apollia n'embarque aucune défense
contre l'injection de prompt, et rien ici ne doit être lu comme tel. Le crate
contient aussi un `InjectionDetector`, qui fait partie du moteur de
permissions qui ne s'exécute pas.

### Autocontrôle sur le chemin orchestré

Sur le chemin d'exécution orchestré, une exécution terminée peut être vérifiée
par un critique avant que son résultat ne soit accepté, sous condition du
palier d'autonomie. Le verdict est émis comme un événement runtime et atterrit
dans le journal signé, si bien que le contrôle fait partie de
l'enregistrement. En cas de verdict négatif, le moteur peut replanifier et
relancer l'exécution, dans la limite d'un nombre borné de tentatives, sous le
même budget partagé : c'est ainsi qu'un agent orchestré se corrige lui-même
sans échapper à son plafond.

Une limite honnête à signaler : cette passe exécute aujourd'hui le critique
LLM ; faire tourner sous gouvernance les propres contrôles shell déclarés par
un agent est une étape ultérieure, pas encore câblée. Le critique est actif ;
les contrôles shell déterministes restent à venir.

## Correspondance avec l'AI Act européen

Les contrôles ci-dessus s'alignent sur les obligations que l'AI Act européen
impose aux systèmes d'IA à haut risque. Apollia **fournit les primitives
techniques qui soutiennent** ces exigences. Il ne vous rend pas conforme et ne
certifie rien : la conformité est un jugement porté par votre organisation et
ses auditeurs sur l'ensemble de votre système et de vos processus, pas une
propriété qu'un runtime peut accorder.

Avec ce cadrage, la correspondance est directe :

| Exigence (thème) | Primitive Apollia |
|---|---|
| Article 10, provenance et qualité des données | le journal d'audit signé et chaîné par hachage, plus sa vérification, qui enregistrent et permettent de confirmer quelles données et quelles actions une exécution a touchées |
| Article 14, supervision humaine | les règles de permission persistées, le garde-fou de l'exécuteur de code, les approbations humaines dans la boucle **sur le chemin du chat**, et les paliers d'autonomie, qui gardent une personne au contrôle des actions à conséquences. Les propres appels d'outil d'un agent installé sont en dehors de cette boucle, voir ci-dessus |
| Article 16, documentation et traçabilité | le journal d'audit et la trace d'exécution, qui documentent ce qui s'est passé |

L'intérêt est que ces mécanismes sont câblés dans le runtime et démontrables
dès aujourd'hui, pas promis. Ce qui reste une responsabilité humaine, c'est de
décider si votre usage de ces mécanismes, dans votre contexte, satisfait
l'obligation. Apollia vous donne le mécanisme ; le jugement de conformité vous
revient.

## Pourquoi c'est le fond du sujet

L'autonomie sans redevabilité est un passif, et une redevabilité rajoutée
après coup n'est pas crédible. La position d'Apollia est que la gouvernance
fait partie du runtime : bornée par des budgets, enregistrée dans une piste
signée que l'on peut vérifier, et supervisée par des permissions et des
paliers. C'est ce qui rend défendable le fait de déléguer un vrai travail à un
agent autonome.

## Voir aussi

- [Auditer et vérifier une exécution](/how-to/audit-and-verify) pour le
  déroulé pratique.
- [Intégrer Apollia par fédération (MCP + REST)](/how-to/embed-via-federation)
  pour la façon dont ces contrôles se propagent dans une intégration hôte.
- La [référence CLI](/reference/cli) pour les commandes d'audit et de
  permissions.
