---
sidebar_position: 6
title: Variables d'environnement
---

# Variables d'environnement

Ce que le runtime lit dans son environnement, et ce qu'il en fait. Tout ce qui
n'est pas répertorié ici est soit résolu au moment de la compilation, soit
réservé aux tests.

L'essentiel de la configuration a sa place dans `apollia.toml`, voir
[Configuration](/reference/configuration). Les variables d'environnement
couvrent trois cas que ce fichier ne peut pas traiter : un secret que vous ne
voulez pas stocker sur disque, une surcharge ponctuelle au lancement, et un
chemin qui dépend de la machine.

## Moteur d'inférence local

Lues à chaque démarrage du moteur. Voir
[Accélérer l'inférence locale](/how-to/accelerate-local-inference) pour savoir
quoi régler et pourquoi.

| Variable | Valeur par défaut | Effet |
|---|---|---|
| `APOLLIA_LLAMA_SERVER_BIN` | résolu depuis `PATH` | Chemin absolu vers le binaire `llama-server`. |
| `APOLLIA_LLAMA_MODEL_PATH` | depuis le backend configuré | Remplace le GGUF chargé par le moteur. |
| `APOLLIA_LLAMA_MAX_LOADED` | voir la valeur par défaut du code source | Nombre de modèles pouvant rester résidents en mémoire simultanément. |
| `APOLLIA_LLAMA_N_CTX` | `32768` | Fenêtre de contexte en tokens. La valeur par défaut est fixe, elle n'est pas lue dans le modèle. |
| `APOLLIA_LLAMA_N_GPU_LAYERS` | `999` | Nombre de couches déchargées sur le GPU ; `0` force l'exécution sur CPU. |
| `APOLLIA_LLAMA_N_BATCH` | valeur par défaut du moteur | Taille de batch logique. |
| `APOLLIA_LLAMA_N_UBATCH` | valeur par défaut du moteur | Taille de micro-batch physique. |
| `APOLLIA_LLAMA_N_PARALLEL` | `1` | Nombre d'emplacements de décodage servis en parallèle. |
| `APOLLIA_LLAMA_CONT_BATCHING` | `true` | Batching continu. |
| `APOLLIA_LLAMA_CACHE_TYPE_K` | valeur par défaut du moteur | Quantification du cache KV, clés. |
| `APOLLIA_LLAMA_CACHE_TYPE_V` | valeur par défaut du moteur | Quantification du cache KV, valeurs. |
| `APOLLIA_LLAMA_FLASH_ATTN` | `on` | Mode flash attention. |
| `APOLLIA_LLAMA_CACHE_REUSE` | valeur par défaut du moteur | Seuil de réutilisation de préfixe. |
| `APOLLIA_LLAMA_METRICS` | `false` | Expose le point de terminaison de métriques du moteur. |
| `APOLLIA_LLAMA_EXTRA_ARGS` | vide | Options supplémentaires transmises telles quelles. |

## Stockage des secrets

| Variable | Valeur par défaut | Effet |
|---|---|---|
| `APOLLIA_TOKEN_STORAGE` | `keyring` | `keyring` utilise le trousseau du système d'exploitation. `file` stocke les secrets sous forme de fichiers chiffrés avec `age` dans `~/.apollia/secrets/`, pour un hôte Linux sans interface où aucun démon de trousseau n'est accessible. |
| `APOLLIA_TOKEN_PASSPHRASE` | aucune | Phrase secrète pour le backend `file`. **Obligatoire quand `APOLLIA_TOKEN_STORAGE=file`** : le démarrage échoue immédiatement en son absence, plutôt que de se replier sur une solution moins sûre. |

## Clients OAuth des connecteurs

Les deux connecteurs diffèrent, et les variables ci-dessous se comportent en
conséquence.

**Microsoft** est prêt à l'emploi dès la livraison : Apollia enregistre une
application cliente publique et embarque son identifiant, si bien qu'aucune
configuration n'est nécessaire. Un client public ne détient aucun secret, ce
qui rend l'embarquement de l'identifiant sans risque ; voir
[Se connecter à Microsoft 365](/operator-help/integrations/connecter-microsoft-365).

**Google** est livré **sans** client, et aucune version publiée n'en embarque
un. Vous enregistrez votre propre application et fournissez ses identifiants
à Apollia, car Google exige un écran de consentement vérifié et un secret
client qu'un binaire distribué ne peut pas détenir. Voir
[Se connecter à Google Workspace](/operator-help/integrations/connecter-google-workspace)
et [Configurer un client OAuth Google](/how-to/set-up-a-google-oauth-client).

Dans les deux cas, la voie prise en charge est Réglages → Intégrations OAuth,
qui écrit dans `~/.apollia/oauth-clients.toml`.

<!-- claim:oauth-client-resolution-order -->
Les variables ci-dessous constituent la troisième voie d'accès, prioritaire
sur ce fichier, pour une session shell, un job CI, ou un hôte sans interface.
Elles sont lues au démarrage du processus, donc elles n'atteignent qu'une
instance d'Apollia lancée depuis le shell qui les a exportées. Ordre de
résolution pour chaque identifiant : variable d'environnement, puis
`oauth-clients.toml`, puis la constante définie à la compilation.

| Variable | Effet |
|---|---|
| `APOLLIA_GOOGLE_CLIENT_ID` | Identifiant client Google. Requis pour connecter Google. |
| `APOLLIA_GOOGLE_CLIENT_SECRET` | Secret associé. Également requis : le type de client Desktop de Google l'exige au point de terminaison de jeton, malgré PKCE. |
| `APOLLIA_GOOGLE_API_KEY` | Clé API pour les appels Google qui utilisent une clé plutôt qu'OAuth (Drive Picker). |
| `APOLLIA_MICROSOFT_CLIENT_ID` | Identifiant d'application (client) Microsoft. **Optionnel**, et il remplace l'identifiant livré par Apollia. Ne le définissez que pour pointer le connecteur vers votre propre enregistrement Entra ; l'exporter vide laisse en place l'identifiant propre à Apollia. |
| `APOLLIA_MICROSOFT_CLIENT_SECRET` | Secret associé. Laissez-le non défini : un client public Microsoft n'en porte aucun, et en envoyer un fait échouer l'échange. |
| `APOLLIA_MICROSOFT_API_KEY` | Clé API, même rôle que ci-dessus. Inutilisée à ce jour. |
| `APOLLIA_FIGMA_CLIENT_ID` | Identifiant client pour le connecteur Figma. |

## Diagnostics

| Variable | Valeur par défaut | Effet |
|---|---|---|
| `APOLLIA_PERF_TRACE` | non définie | Chemin d'un fichier recevant un enregistrement de performance par tour. Non définie signifie qu'aucun fichier n'est écrit et qu'aucune provenance n'est collectée ; le résumé est de toute façon émis au niveau `INFO`. |
| `APOLLIA_MCP_PROTOCOL_VERSION` | figée dans le code | Remplace la révision du protocole MCP annoncée à un serveur. Sert à sonder un serveur qui fige une révision différente, pas pour un usage normal. |
| `RUST_LOG` | `apollia=info` | Filtre `tracing` standard. C'est `apollia=trace` qui rend visible `[llm.observability] debug_log_prompt` ; voir [Configuration](/reference/configuration). |

## Agent compagnon embarqué

Surcharges utilisées lors du développement de l'agent compagnon livré avec
l'application desktop. Elles pointent le runtime vers une copie de travail
plutôt que vers la copie embarquée.

`APOLLIA_GUIDE_PY`, `APOLLIA_GUIDE_TOML`, `APOLLIA_GUIDE_CAPABILITIES_MD`,
`APOLLIA_GUIDE_TUTORIALS_MD`, `APOLLIA_GUIDE_VERSION`.

## Automatisation desktop

`APOLLIA_AUTOMATION`, `APOLLIA_AUTOMATION_OUT` et
`APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE` pilotent le harnais de test gestuel
réservé au développement. Elles n'ont aucun effet dans une version de
production, où le harnais est exclu à la compilation.

## Non lues par le runtime

Les variables `APOLLIA_BUILD_*` sont lues au moment de la compilation, pas à
l'exécution ; en définir une avant de lancer Apollia n'a aucun effet.

Elles offrent un point d'accroche pour recompiler Apollia depuis les sources
avec votre propre application enregistrée, un déploiement en flotte étant le
cas d'usage évident : définissez-les dans l'environnement de compilation, et
le binaire obtenu embarque ces identifiants, si bien que les machines sur
lesquelles il atterrit n'ont besoin d'aucune configuration propre à l'hôte.

**Aucune version publiée d'Apollia ne les définit.** Pour Google, cela
signifie que la valeur compilée est vide dans chaque version publiée, et que
les deux sources d'exécution ci-dessus sont les seules à se résoudre. Pour
Microsoft, la valeur compilée n'est pas vide pour autant : elle provient
d'une constante du code source, pas de ces variables, ce qui explique
pourquoi définir `APOLLIA_BUILD_MICROSOFT_CLIENT_ID` n'est jamais nécessaire,
sauf pour remplacer l'enregistrement d'Apollia au moment de la compilation.
