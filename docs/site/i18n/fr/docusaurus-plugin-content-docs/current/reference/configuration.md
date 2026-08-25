---
sidebar_position: 4
title: Configuration (apollia.toml)
---

# Configuration (apollia.toml)

Référence de la surface de configuration `apollia.toml`.

Le runtime recherche `apollia.toml` d'abord dans le répertoire de travail, puis
dans `~/.config/apollia/apollia.toml`. Chaque section est optionnelle : une
section absente retombe sur ses valeurs par défaut. Les données d'exécution
(bases de données, jeton API, modèles) vivent séparément sous `~/.apollia/`.

## Sections

Huit sections sont lues. Une section en dehors de cette liste est ignorée, et
`apollia-os config set` la rejette plutôt que d'accepter une valeur que rien ne
consultera jamais.

| Section | Objet |
|---|---|
| `[llm]` | Configuration du backend LLM. |
| `[api]` | Écouteur TCP et authentification (`bind`, `require_token`, `tls_cert`, `tls_key`). |
| `[runtime]` | Capacités de l'EventBus et des mailbox. |
| `[hitl]` | Délai d'expiration et intervalle de scan du human-in-the-loop. |
| `[tools]` | Outils natifs : limites, désactivation statique, et configuration par outil `[tools.web_search]` / `[tools.web_read]`. |
| `[mcp]` | Configuration du module MCP, y compris `[[mcp.servers]]` (voir plus bas). |
| `[hooks]` | Gestionnaires de hooks de cycle de vie (commande ou HTTP). |
| `[chat]` | Valeurs par défaut au niveau session du sous-système de chat (par exemple `plan_mode_default`). |

L'application desktop lit une section supplémentaire, `[observability]`,
documentée plus bas. Elle n'est pas modifiable depuis la CLI.

Les tableaux ci-dessous sont générés à partir des types Rust, si bien qu'aucun
champ ne peut s'en écarter. Ils couvrent le **niveau racine de chaque section**.
Une table imbriquée, comme une entrée de `[[llm.backends]]` ou
`[[mcp.servers]]`, a ses propres champs : celle de MCP est documentée
intégralement plus bas, les autres se lisent depuis les types qu'elles
nomment.

<!-- BEGIN GENERATED: config-fields -->

### `[llm]`

Backends LLM et routage.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `default` | `String` | **requis** | Nom du backend par défaut (doit exister dans `backends`). |
| `backends` | `Vec<BackendConfig>` | **requis** | Backends à instancier depuis `[[llm.backends]]`. |
| `observability` | `ObservabilityConfig` | défaut du type | Réglages d'observabilité (tokens, latence, coût, debug des prompts). |
| `routing` | `Option<LlmRoutingConfig>` | `None` | Routage LLM par niveau de précision (section `[llm.routing]`). |
| `pricing_overrides` | `HashMap<String, PricingTier>` | vide | Surcharges de tarification par l'opérateur (section `[llm.pricing_overrides]`). |
| `cost_alert_threshold_usd` | `Option<f64>` | `None` | Seuil de coût en USD au-delà duquel [`RuntimeEvent::TokenBudgetUpdated`] est émis avec `threshold_exceeded = true`. |
| `vertex` | `Option<VertexConfig>` | `None` | Configuration optionnelle du backend Google Vertex AI (`[llm.vertex]`). |
| `runner` | `LlmRunnerConfig` | défaut du type | Configuration du sidecar LLM local (section `[llm.runner]`). |

### `[runtime]`

Capacités de l'EventBus et des mailbox.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `eventbus_capacity` | `usize` | `1024` | Capacité du canal de diffusion (broadcast) de l'EventBus. |
| `mailbox_capacity` | `usize` | `100` | Capacité maximale de la mailbox d'un acteur. |
| `mailbox_visibility_timeout_secs` | `u64` | `60` | Délai de visibilité d'un message de mailbox pris en bail (leased), en secondes. |
| `mailbox_message_ttl_secs` | `u64` | `86_400` | Durée de vie d'un message de mailbox jamais reçu, en secondes. |
| `mailbox_send_quota_per_run` | `u32` | `50` | Nombre maximal d'envois de mailbox autorisés par exécution (garde-fou anti-spam). |
| `mailbox_max_payload_bytes` | `usize` | `65_536` | Taille maximale sérialisée d'un message de mailbox, en octets. |
| `mailbox_audit_full_payload` | `bool` | `false` | Indique si le journal d'audit enregistre la charge utile complète du message. |
| `startup_timeout_secs` | `u64` | `300` | Délai de démarrage du runtime, en secondes. |

### `[api]`

Écouteur TCP, authentification, TLS, socket Unix.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `bind` | `String` | `"127.0.0.1".to_owned()` | Adresse IP sur laquelle lier l'écouteur TCP. |
| `port` | `u16` | `7771` | Port TCP du serveur REST. |
| `require_token` | `bool` | `true` | Exige un jeton Bearer sur chaque connexion TCP entrante. |
| `unix_socket` | `PathBuf` | `~/.apollia/runtime.sock` | Chemin du socket Unix local. Le serveur le passe en `0600` après le bind. |
| `tls_cert` | `Option<PathBuf>` | `None` | Chaîne de certificats PEM pour le TLS natif sur l'écouteur TCP. |
| `tls_key` | `Option<PathBuf>` | `None` | Clé privée PEM correspondant à [`tls_cert`](Self::tls_cert). |

### `[hitl]`

Délai d'expiration et intervalle de scan du human-in-the-loop.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `timeout_hours` | `Option<u64>` | `None` | Attente maximale d'une approbation humaine, en heures. |
| `scan_interval_secs` | `u64` | `60` | Intervalle de scan des tâches HITL expirées, en secondes. |

### `[tools]`

Outils natifs : désactivation statique et réglages par outil.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `max_output_chars` | `usize` | `30_000` | Taille maximale d'une sortie d'outil transmise au LLM, en octets UTF-8. |
| `file_path_extraction_pattern` | `Option<String>` | `None` | Motif regex pour extraire des chemins depuis une sortie bash. |
| `disabled` | `Vec<String>` | vide | Outils natifs désactivés statiquement par l'opérateur dans `apollia.toml`. |
| `web_search` | `WebSearchConfig` | défaut du type | Configuration de l'outil natif `web_search`. |
| `web_read` | `WebReadConfig` | défaut du type | Configuration de l'outil natif `web_read`. |

### `[mcp]`

Client MCP : chargement des outils et limites de réponse.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `approval_ttl_hours` | `u64` | `24` | Durée de validité des approbations HITL MCP, en heures. |
| `tool_loading` | `McpToolLoading` | défaut du type | Stratégie de chargement des schémas d'outils pour tous les serveurs MCP. |
| `tool_search_limit` | `usize` | `20` | Nombre maximal de résultats renvoyés par l'outil synthétique `tool_search`. |

### `[hooks]`

Gestionnaires de hooks de cycle de vie.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `handlers` | `Vec<HookHandlerConfig>` | vide | Gestionnaires de hooks enregistrés. |

### `[chat]`

Valeurs par défaut de session de chat.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `plan_mode_default` | `bool` | `false` | État par défaut du mode plan hérité par chaque nouvelle session de chat. |
| `default_workspace` | `Option<String>` | `None` | Répertoire de travail par défaut pour les sessions de chat libre (sans projet). |
| `tool_turn_temperature` | `Option<f32>` | `None` | Température LLM appliquée à un tour de chat qui expose des outils au modèle. |

### `[observability]`

Capture et rétention des traces. Lu par l'application desktop uniquement.

| Clé | Type | Valeur par défaut | Signification |
| --- | --- | --- | --- |
| `max_input_bytes` | `usize` | `DEFAULT_MAX_INPUT_BYTES` | Taille maximale des entrées de tâche/étape, en octets (défaut 32768). |
| `max_output_bytes` | `usize` | `DEFAULT_MAX_OUTPUT_BYTES` | Taille maximale des sorties de tâche/étape/complétion, en octets (défaut 32768). |
| `max_tool_output_bytes` | `usize` | `DEFAULT_MAX_TOOL_OUTPUT_BYTES` | Taille maximale du stdout/stderr d'un outil, en octets (défaut 10240). |
| `capture_thoughts` | `bool` | `true` | Si `true`, persiste les enregistrements `Thought` ReAct sur la trace (défaut `true`). Désactiver vide les bulles de raisonnement dans l'UI builder. |
| `capture_tool_args` | `bool` | `true` | Si `true`, persiste le `args_json` complet des appels d'outils (défaut `true`). Désactiver ne laisse visible que le nom de l'outil et sa durée. |
| `capture_tool_outputs` | `bool` | `true` | Si `true`, persiste l'`output_json` complet des appels d'outils (défaut `true`). Désactiver ne laisse visible que le succès ou l'échec. |
| `capture_agent_logs` | `bool` | `true` | Si `true`, persiste les appels Python `ctx.log()` sur la trace (défaut `true`). Désactiver laisse `tracing::*` fonctionner mais n'écrit plus rien dans `runtime_events.db`. |
| `retention_days` | `u32` | `90` | Durée de rétention en jours des `runtime_events` avant purge automatique (défaut 90, cohérent avec audit.db). |
<!-- END GENERATED: config-fields -->

### Sections retirées

`[a2a]`, `[oria]`, `[registry]`, `[permissions]`, `[filesystem]`, `[memory]` et
`[budget]` étaient acceptées auparavant. Chacune se désérialisait dans une
structure typée que rien ensuite ne consultait, si bien qu'écrire une valeur
dedans n'avait aucun effet et ne produisait pas non plus d'erreur. Elles ne
sont plus acceptées, et un fichier qui en porte encore une consigne un
avertissement au démarrage. Les retirer n'a changé aucun comportement, puisque
ces sections n'en avaient aucun.

`[permissions]` mérite d'être détaillée, car son nom suggère le contraire :
aucune de ses quatre clés n'a jamais eu de lecteur sur un chemin d'exécution. La
gouvernance qui s'exécute réellement, règles de préfixe et approbations, ne
prend rien de cette section. Voir
[concepts transverses](/architecture/crosscutting-concepts).

Les paramètres d'échantillonnage sont documentés séparément dans
[Valeurs par défaut d'échantillonnage](/reference/sampling-defaults). Les clés
`[tools.web_search]` et `[tools.web_read]` sont aussi modifiables depuis la CLI
avec `apollia-os tools config set <tool>.<key> <value>`.

## Capture de trace (`[observability]`)

Lu par l'application desktop uniquement ; le démon CLI utilise les valeurs par
défaut. Modifiable depuis Réglages, Observabilité, ou à la main.

Ces interrupteurs décident de ce qu'une exécution d'agent laisse sur disque,
dans `runtime_events.db`. Tout reste sur la machine, la question n'est donc pas
qui d'autre le voit, mais ce qui reste lisible localement après une
exécution.

<!-- claim:observability-capture-switches-enforced -->

| Clé | Valeur par défaut | Effet |
|---|---|---|
| `capture_thoughts` | `true` | Persiste le texte de raisonnement de chaque tour ReAct. Désactivé, le tour ne laisse aucune ligne de pensée. |
| `capture_agent_logs` | `true` | Persiste les messages émis via `ctx.logger`. Désactivé, aucune ligne de log. |
| `capture_tool_args` | `true` | Persiste le JSON des arguments de chaque appel d'outil. Désactivé, l'appel reste tracé, mais sans ses arguments. |
| `capture_tool_outputs` | `true` | Persiste le JSON de sortie de chaque appel d'outil. Désactivé, l'appel et son résultat restent liés, mais sans le contenu. |
| `retention_days` | `90` | Nombre de jours d'événements conservés. La purge s'exécute au démarrage et ne supprime rien d'autre : la piste d'audit et le journal d'audit signé sont des magasins séparés. `0` signifie ne jamais purger. |
| `max_input_bytes` | `32768` | Seuil de troncature pour une entrée de tâche persistée. |
| `max_output_bytes` | `32768` | Seuil de troncature pour une sortie de tâche persistée. |

<!-- claim:retention-purges-runtime-events-only -->
Désactiver un interrupteur vide la partie correspondante de la timeline : la
piste d'audit et l'historique des coûts sont séparés et n'en sont pas
affectés. La même séparation vaut pour la rétention : la purge ne supprime que
depuis le journal d'événements. Le journal d'audit signé est une chaîne de
hachage que `audit verify` parcourt de bout en bout, il n'est donc jamais
purgé sur une base temporelle.

### Contenu des prompts (`[llm.observability]`)

<!-- claim:debug-log-prompt-logs-at-trace -->

Le texte du prompt n'est **jamais écrit dans une base de données**. Le seul
réglage capable de l'exposer est `debug_log_prompt`, et il vit sous
`[llm.observability]`, pas sous `[observability]` :

| Clé | Valeur par défaut | Effet |
|---|---|---|
| `debug_log_prompt` | `false` | Émet le prompt complet au niveau `TRACE`, sur le chemin de complétion comme sur le chemin de streaming. Rien n'est persisté ; l'exposition dépend de ce qui collecte le flux de logs. |

**L'interrupteur seul ne montre rien.** Le filtre de log par défaut est
`apollia=info`, et `TRACE` se situe en dessous, si bien que le prompt est émis
à un niveau qui est filtré. Le voir exige à la fois `debug_log_prompt = true`
et un filtre au niveau trace, par exemple `RUST_LOG=apollia=trace`. C'est un
second verrou délibéré, pas un oubli, et c'est pourquoi le réglage peut sans
risque rester visible dans l'interface.

Un `debug_log_prompt` écrit sous `[observability]` n'est lu par rien : les
deux sections se désérialisent en deux types différents, et seul celui de
`[llm.observability]` atteint le routeur.

### Clés déclarées mais non implémentées

| Clé | État |
|---|---|
| `capture_thoughts` sur les étapes de plan ORIA | Partiel. Les entrées et sorties des étapes de plan sont persistées avec les valeurs par défaut compilées, si bien que les limites d'octets ci-dessus ne s'y appliquent pas. |
| `max_tool_output_bytes` | **Non implémenté.** N'a jamais eu de point d'usage. |

Ces clés sont listées plutôt que masquées parce que la page de réglages en
affiche encore certaines. Un interrupteur qui ressemble à un contrôle de
confidentialité et ne fait rien est pire qu'un interrupteur absent, donc tant
qu'elles ne sont pas implémentées ou retirées, ce tableau fait foi.

## Dictée (`system.db`, pas `apollia.toml`)

La dictée vocale n'a pas de section `apollia.toml`. Ses dix réglages vivent
dans une unique ligne de `~/.apollia/system.db`, écrite depuis Réglages,
Speech-to-Text dans l'application desktop, ou avec
`apollia-os stt config get` et `apollia-os stt config update`. Écrire un bloc
`[stt]` dans `apollia.toml` ne change rien : rien ne le lit.

<!-- claim:stt-settings-apply-without-restart -->

Sauvegarder réarme le flux de capture, si bien qu'un changement prend effet
dès la prochaine dictée sans redémarrer l'application.

| Clé | Valeur par défaut | Effet |
|---|---|---|
| `enabled` | `false` | Indique si le moteur de dictée démarre et si le raccourci global est armé. |
| `model_path` | *(vide)* | Fichier de modèle Whisper. `~` est développé. L'application desktop scrute `~/.apollia/models` pour les fichiers `.bin` et `.gguf`. |
| `hotkey` | `ctrl+shift+space` | Raccourci global qui démarre et arrête la dictée. |
| `trigger_mode` | `toggle` | `toggle` (appui pour démarrer, appui pour arrêter) ou `push-to-talk` (maintien). |
| `input_device` | *(non défini)* | Nom du microphone tel que rapporté par le système. Non défini signifie l'entrée par défaut du système. |
| `language` | *(non défini)* | Langue forcée sur le moteur. Non défini signifie détection automatique. Valeurs acceptées ci-dessous. |
| `silence_threshold_db` | `-40.0` | Niveau RMS, en dB, en dessous duquel une fenêtre de 10 ms compte comme du silence. |
| `max_recording_sec` | `60` | Durée maximale d'enregistrement conservée. Au-delà, l'audio est tronqué. |
| `clipboard_mode` | `paste` | `paste`, `clipboard`, `memo` ou `both`. S'applique uniquement à la dictée par raccourci. |
| `clipboard_restore` | `true` | Restaure le contenu précédent du presse-papiers après un collage. |

### Codes de langue acceptés

<!-- claim:stt-language-hint-is-a-closed-list -->

`language` est un code ISO 639-1 issu de cette liste fermée, ou non défini
pour la détection automatique. L'application desktop propose exactement ces
langues dans un sélecteur ; une valeur en dehors de la liste est rejetée
plutôt que transmise telle quelle, si bien que deux machines ne peuvent pas
finir avec des orthographes différentes de la même langue.

| Code | Langue | Code | Langue |
|---|---|---|---|
| `fr` | Français | `pl` | Polonais |
| `en` | Anglais | `ru` | Russe |
| `es` | Espagnol | `zh` | Chinois |
| `de` | Allemand | `ja` | Japonais |
| `it` | Italien | `ko` | Coréen |
| `pt` | Portugais | `ar` | Arabe |
| `nl` | Néerlandais | | |

<!-- claim:stt-api-language-is-per-request -->

`POST /stt/transcribe` accepte aussi un champ `language`, qui ne s'applique
qu'à cette requête et écrase la valeur enregistrée ; l'envoyer vide signifie
détection automatique pour cette requête.

### Le silence n'est pas transcrit

<!-- claim:stt-refuses-silent-audio -->

Un enregistrement dont chaque fenêtre de 10 ms se situe sous
`silence_threshold_db` est écarté plutôt qu'envoyé au modèle. Ce n'est pas une
optimisation. Whisper ne répond pas au silence par une chaîne vide, il répond
par un remplissage appris de ses données d'entraînement, et ces inventions
arrivaient auparavant comme s'il s'agissait de transcriptions réelles.
L'interface signale que rien d'audible n'a été capté, et la ligne de log
`stt.audio.nothing_audible` enregistre le niveau de crête mesuré, ce qui
permet de distinguer un microphone coupé d'un seuil réglé trop haut.

## Serveurs MCP (`[[mcp.servers]]`)

Chaque entrée configure un serveur MCP. Les limites pertinentes pour la
sécurité :

### `max_response_bytes`

Nombre maximal d'octets acceptés depuis une seule réponse de serveur avant que
le transport n'interrompe la lecture avec une erreur.

- Type : entier (octets)
- Valeur par défaut : `8388608` (8 Mio)
- Bornes : `1024` à `1073741824` (1 Kio à 1 Gio)
- S'applique aux transports : `stdio`, `streamable-http` et `sse`

Les serveurs MCP sont non fiables. Un serveur qui ne termine jamais une ligne,
qui diffuse sans fin, ou qui renvoie un corps surdimensionné ferait sinon
croître la mémoire du démon sans limite. Le plafond borne une seule ligne
stdio, une lecture de corps HTTP, et le tampon de réception SSE. Augmentez-le
pour les serveurs dont les charges utiles d'outils sont légitimement
volumineuses.

### `max_tools`

Nombre maximal d'outils conservés depuis la liste d'outils d'un serveur. Les
outils au-delà du plafond sont écartés à la découverte.

- Type : entier (compte)
- Valeur par défaut : `256`
- Bornes : `1` à `8192`

Les serveurs MCP sont non fiables. Un serveur annonçant des milliers d'outils
saturerait sinon le registre d'outils et le catalogue d'outils du modèle,
épuisant contexte et mémoire. Les noms d'outils sont eux aussi validés
(écartés sauf s'ils correspondent à `[A-Za-z0-9_.-]`) et les descriptions
d'outils sont dépouillées de leurs caractères de contrôle, si bien qu'un
serveur ne peut ni forger de lignes de log ni glisser d'instructions dans le
contexte du modèle. Augmentez `max_tools` pour les serveurs d'agrégation qui
exposent légitimement de nombreux outils.

```toml
[[mcp.servers]]
name = "example"
transport = "streamable-http"
url = "https://mcp.example.com/mcp"
max_response_bytes = 16777216  # 16 Mio
max_tools = 512
```
