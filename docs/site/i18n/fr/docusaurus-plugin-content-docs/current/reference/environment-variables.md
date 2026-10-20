---
sidebar_position: 6
title: Variables d'environnement
---

# Variables d'environnement

Ce que le runtime lit dans son environnement, et ce qu'il en fait. Les tableaux
ci-dessous couvrent les noms `APOLLIA_*` définis par Apollia. Apollia lit aussi
des variables standard qui ne lui appartiennent pas, là où leur sens habituel
s'applique : `EDITOR` quand `apollia-os config edit` ouvre `apollia.toml`,
`XDG_CONFIG_HOME` quand `apollia-os doctor` cherche un fichier de configuration
resté à un ancien emplacement et quand `apollia-os service install` écrit une
unité utilisateur systemd, `NO_COLOR` pour la sortie terminal, `PATH` quand elle
cherche un binaire, `TZ`, et `USERNAME` sous Windows pour dériver le nom du
tuyau nommé. Un backend compatible OpenAI lit la variable que nomme son
`api_key_env`.

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
| `APOLLIA_LLAMA_SERVER_BIN` | le moteur livré avec l'artefact | Chemin absolu vers un binaire `llama-server`, prioritaire sur celui embarqué. C'est la façon d'utiliser une compilation à vous, une compilation CUDA sous Linux par exemple. |
| `APOLLIA_LLAMA_MAX_LOADED` | *(non définie)* | Un plafond explicite du nombre de modèles résidents en mémoire simultanément. Sans elle, le garde-fou de chargement décide : un modèle est jugé avec l'empreinte des modèles déjà résidents ajoutée à la sienne (voir `[llm.guardrails]` dans la référence de configuration). Une valeur nulle ou illisible est ignorée. |
| `APOLLIA_LLAMA_N_CTX` | *(non définie)* | Fenêtre de contexte en tokens, au-dessus de toute autre source. Non définie, la fenêtre est celle du preset, celle du backend, puis le défaut de `[llm.defaults]` dans `apollia.toml`, et 32768 en dernier. |
| `APOLLIA_LLAMA_N_GPU_LAYERS` | calculé, sinon `999` | Nombre de couches déchargées sur le GPU ; `0` force l'exécution sur CPU. Non définie, le daemon calcule ce nombre à partir du fichier du modèle sur un GPU dédié, et transmet `999` sur Apple Silicon ou sans GPU. |
| `APOLLIA_LLAMA_N_BATCH` | valeur par défaut du moteur | Taille de batch logique. |
| `APOLLIA_LLAMA_N_UBATCH` | valeur par défaut du moteur | Taille de micro-batch physique. |
| `APOLLIA_LLAMA_N_PARALLEL` | planifié | Nombre d'emplacements de décodage servis en parallèle. Non définie, le nombre est planifié d'après le modèle et la mémoire, jusqu'à `8` ; définie, elle est prise telle quelle. |
| `APOLLIA_LLAMA_CONT_BATCHING` | `true` | Batching continu. |
| `APOLLIA_LLAMA_CACHE_TYPE_K` | valeur par défaut du moteur | Quantification du cache KV, clés. |
| `APOLLIA_LLAMA_CACHE_TYPE_V` | valeur par défaut du moteur | Quantification du cache KV, valeurs. |
| `APOLLIA_LLAMA_FLASH_ATTN` | `on` | Mode flash attention. |
| `APOLLIA_LLAMA_CACHE_REUSE` | valeur par défaut du moteur | Seuil de réutilisation de préfixe. |
| `APOLLIA_LLAMA_METRICS` | `false` | Expose le point de terminaison de métriques du moteur. |
| `APOLLIA_LLAMA_LOG_VERBOSITY` | `4` | Niveau de journal du moteur (`-lv`) ; à `4`, le chargement nomme son périphérique et le nombre de couches déchargées. L'emporte sur `[engines] log_level` d'`apollia.toml` (Réglages > Développeur). |
| `APOLLIA_LLAMA_EXTRA_ARGS` | vide | Options supplémentaires transmises telles quelles. |

Le GGUF chargé par le moteur n'a délibérément aucune surcharge par
l'environnement. C'est un état que possède le changement de modèle dans
l'interface, et une variable ambiante qui le figerait annulerait chaque
changement fait depuis celle-ci.

## Stockage des secrets

| Variable | Valeur par défaut | Effet |
|---|---|---|
| `APOLLIA_TOKEN_STORAGE` | `keyring` | `keyring` utilise le trousseau du système d'exploitation. `file` stocke les secrets sous forme de fichiers chiffrés avec `age` dans `~/.apollia/secrets/`, pour un hôte Linux sans interface où aucun démon de trousseau n'est accessible. Les jetons OAuth des connecteurs suivent ce commutateur comme tous les autres secrets : avec `file`, les jetons Google et Microsoft y sont écrits aussi. Toute autre valeur est refusée. |
| `APOLLIA_TOKEN_PASSPHRASE` | aucune | Phrase secrète pour le backend `file`. **Obligatoire quand `APOLLIA_TOKEN_STORAGE=file`** : le démarrage échoue immédiatement en son absence, plutôt que de se replier sur une solution moins sûre. |

## Clients OAuth des connecteurs

Les deux connecteurs diffèrent, et les variables ci-dessous se comportent en
conséquence.

**Microsoft** est prêt à l'emploi dès la livraison : Apollia enregistre une
application cliente publique et embarque son identifiant, si bien qu'aucune
configuration n'est nécessaire. Un client public ne détient aucun secret, ce
qui rend l'embarquement de l'identifiant sans risque ; voir
[Se connecter à Microsoft 365](/operator-help/integrations/connect-microsoft-365).

**Google** est livré **sans** client, et aucune version publiée n'en embarque
un. Vous enregistrez votre propre application et fournissez ses identifiants
à Apollia, car Google exige un écran de consentement vérifié et un secret
client qu'un binaire distribué ne peut pas détenir. Voir
[Se connecter à Google Workspace](/operator-help/integrations/connect-google-workspace)
et [Configurer un client OAuth Google](/how-to/set-up-a-google-oauth-client).

Dans les deux cas, la voie prise en charge est la fiche de la connexion, dans
Connexions. L'identifiant client saisi là est écrit dans
`~/.apollia/oauth-clients.toml` ; le secret client et la clé API Google vont
dans le coffre de secrets choisi ci-dessus, jamais dans ce fichier.

<!-- claim:oauth-client-resolution-order -->
Les variables ci-dessous constituent l'autre voie d'accès, prioritaire sur ce
que la fiche a enregistré, pour une session shell, un job CI, ou un hôte sans
interface. Elles sont lues au démarrage du processus, donc elles n'atteignent
qu'une instance d'Apollia lancée depuis le shell qui les a exportées. Ordre de
résolution pour chaque identifiant : variable d'environnement, puis la valeur
enregistrée depuis la fiche (`oauth-clients.toml` pour un identifiant client,
le coffre de secrets pour un secret ou une clé API), puis la constante définie
à la compilation.

| Variable | Effet |
|---|---|
| `APOLLIA_GOOGLE_CLIENT_ID` | Identifiant client Google. Requis pour connecter Google. |
| `APOLLIA_GOOGLE_CLIENT_SECRET` | Secret associé. Également requis : le type de client Desktop de Google l'exige au point de terminaison de jeton, malgré PKCE. |
| `APOLLIA_GOOGLE_API_KEY` | Clé API pour les appels Google qui utilisent une clé plutôt qu'OAuth (Drive Picker). |
| `APOLLIA_MICROSOFT_CLIENT_ID` | Identifiant d'application (client) Microsoft. **Optionnel**, et il remplace l'identifiant livré par Apollia. Ne le définissez que pour pointer le connecteur vers votre propre enregistrement Entra ; l'exporter vide laisse en place l'identifiant propre à Apollia. |
| `APOLLIA_MICROSOFT_CLIENT_SECRET` | Secret associé. Laissez-le non défini : un client public Microsoft n'en porte aucun, et en envoyer un fait échouer l'échange. |
| `APOLLIA_MICROSOFT_API_KEY` | Clé API, même rôle que ci-dessus. Inutilisée à ce jour. |
| `APOLLIA_FIGMA_CLIENT_ID` | Identifiant client pour le connecteur Figma. |

## Connexions

| Variable | Valeur par défaut | Effet |
|---|---|---|
| `APOLLIA_MCP_REGISTRY_URL` | `https://registry.modelcontextprotocol.io` | Adresse du registre MCP dans lequel le catalogue public est lu. Définissez-la pour pointer Apollia vers un registre privé. Une valeur vide laisse la valeur par défaut en place. |
| `APOLLIA_IMAP_PASSWORD` | aucune | Mot de passe d'application lu par `apollia-os connector imap` quand `--password-stdin` n'est pas donné. Le mot de passe n'est jamais pris sur la ligne de commande. |

## Python et le moteur MLX

Des chemins qui dépendent de la machine. Une installation empaquetée les
définit ou les trouve seule ; ils comptent quand Apollia tourne depuis un arbre
de sources ou à côté d'un bundle qui n'est pas là où il l'attend.

| Variable | Valeur par défaut | Effet |
|---|---|---|
| `APOLLIA_BUNDLED_PYTHON` | trouvé à côté de l'exécutable | Chemin absolu de l'interpréteur Python embarqué. L'application desktop l'exporte au démarrage. |
| `APOLLIA_PYTHON_BUNDLE_DIR` | trouvé à côté de l'exécutable | Dossier où se trouve l'interpréteur Python embarqué. Les lanceurs en ligne de commande l'exportent à la place de la variable ci-dessus ; les deux sont lues. |
| `APOLLIA_MLX_ENGINE_DIR` | l'environnement MLX installé le plus récent, sinon celui embarqué | Dossier contenant les paquets du moteur MLX. |
| `APOLLIA_MLX_PYTHON` | l'interpréteur embarqué | Interpréteur Python sur lequel tourne le moteur MLX. |

## Diagnostics

| Variable | Valeur par défaut | Effet |
|---|---|---|
| `APOLLIA_PERF_TRACE` | non définie | Chemin d'un fichier recevant un enregistrement de performance par tour. Non définie signifie qu'aucun fichier n'est écrit et qu'aucune provenance n'est collectée ; le résumé est de toute façon émis au niveau `INFO`. |
| `APOLLIA_PERF_MODEL_SHA256` | non définie | Empreinte du fichier du modèle, écrite telle quelle dans l'enregistrement de performance au lieu d'être calculée. Pour une campagne de mesure qui recharge de nombreuses fois le même gros modèle. |
| `RUST_LOG` | `apollia=info` | Filtre `tracing` standard. C'est `apollia=trace` qui rend visible `[llm.observability] debug_log_prompt` ; voir [Configuration](/reference/configuration). |

## Automatisation desktop

`APOLLIA_AUTOMATION`, `APOLLIA_AUTOMATION_OUT` et
`APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE` pilotent le harnais de test gestuel
réservé au développement. Elles n'ont aucun effet dans une version de
production, où le harnais est exclu à la compilation. `APOLLIA_POLICY_DIR`,
qui pointe le runtime vers un autre dossier de politique de l'organisation,
suit la même règle : elle n'est lue que dans les versions de débogage.

## Non lues par le runtime

Les variables `APOLLIA_BUILD_*` sont lues au moment de la compilation, pas à
l'exécution ; en définir une avant de lancer Apollia n'a aucun effet.
`APOLLIA_PYTHON` est de la même nature : elle nomme l'interpréteur Python que
la compilation de l'application desktop lance pour empaqueter la documentation
que l'application embarque.

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
