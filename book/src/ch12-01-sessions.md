# Sessions et streaming

Une session chat est l'unité de persistance conversationnelle d'Apollia OS. Elle contient l'historique des messages, le mode d'exécution, les outils autorisés, et le statut courant. Une fois créée, elle survit aux redémarrages du runtime.

> **Référence technique :** [Briques-Chat](https://github.com/nidal-z/apollia-os/wiki/Briques-Chat) — endpoints REST complets, événements runtime, schéma SQLite.

---

## Créer une session

```bash
curl -X POST http://localhost:7771/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "mode": "libre",
    "system_prompt": "Tu es un assistant pour l'\''analyse de données."
  }'
```

```json
{
  "id": "cs-a1b2c3",
  "mode": "libre",
  "status": "active",
  "created_at": "2026-04-02T09:00:00Z"
}
```

Pour un Chat Agent, précisez `agent_name` :

```bash
curl -X POST http://localhost:7771/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "mode": "agent",
    "agent_name": "csv-data-worker",
    "system_prompt": "Réponds en français, sois concis."
  }'
```

---

## Envoyer un message

```bash
curl -X POST http://localhost:7771/api/v1/sessions/cs-a1b2c3/messages \
  -H "Content-Type: application/json" \
  -d '{"content": "Quelles colonnes contient le fichier /data/ventes.csv ?"}'
```

```json
{
  "message_id": "msg-001",
  "session_id": "cs-a1b2c3",
  "role": "user",
  "content": "Quelles colonnes contient le fichier /data/ventes.csv ?",
  "seq": 1
}
```

La réponse confirme l'enregistrement du message. La réponse de l'assistant arrive via le stream SSE.

---

## Suivre le stream SSE

```bash
curl -N http://localhost:7771/api/v1/sessions/cs-a1b2c3/stream
```

```
data: {"event":"message_sent","session_id":"cs-a1b2c3","seq":1}
data: {"event":"response_started","session_id":"cs-a1b2c3"}
data: {"event":"token","token":"Le"}
data: {"event":"token","token":" fichier"}
data: {"event":"token","token":" contient"}
data: {"event":"token","token":" 5 colonnes"}
data: {"event":"token","token":" : date"}
data: {"event":"token","token":", montant"}
data: {"event":"token","token":", client"}
data: {"event":"token","token":", région"}
data: {"event":"token","token":", statut."}
data: {"event":"response_completed","session_id":"cs-a1b2c3","seq":2,"tokens":47}
```

Le stream reste ouvert entre les messages — vous recevrez tous les échanges suivants sans reconnecter. Chaque `token` est émis dès qu'il est produit par le LLM.

Les 12 événements runtime du chat (`ChatSessionCreated`, `ChatToken`, `ChatToolCallStarted`, `ChatApprovalRequired`…) sont documentés dans [Briques-Chat](https://github.com/nidal-z/apollia-os/wiki/Briques-Chat).

---

## Approbation d'outil inline (HITL)

Quand un outil est utilisé pour la première fois dans une session, le runtime peut suspendre la génération et émettre `ChatApprovalRequired`. Trois décisions sont possibles : **Autoriser une fois** (`accept`), **Refuser** (`refuse`), ou **Toujours autoriser** (`always_accept`).

```bash
curl -X POST http://localhost:7771/api/v1/sessions/cs-a1b2c3/authorize \
  -H "Content-Type: application/json" \
  -d '{
    "message_id": "msg-003",
    "tool_name":  "file_read",
    "decision":   "always_accept"
  }'
```

La décision `always_accept` est persistée au minimum en SQLite (`chat_tool_authorizations`) pour cette session. Dans l'interface desktop, le bouton **"Toujours autoriser"** ouvre un sélecteur de portée :

| Portée | Persistance |
|---|---|
| Pour cette session | In-memory uniquement — expire à la fermeture du chat |
| Toujours pour cet assistant | Règle `scope=agent` dans `governance.db` |
| Toujours pour ce projet | Règle `scope=project` dans `governance.db` |
| Toujours, partout | Règle `scope=global` dans `governance.db` |

> **Référence technique :** [Briques-Chat](https://github.com/nidal-z/apollia-os/wiki/Briques-Chat) — détail des 4 scopes, persistance et révocation.

---

## Lister et inspecter les sessions

```bash
# Lister toutes les sessions
curl http://localhost:7771/api/v1/sessions

# Détail d'une session avec l'historique complet
curl http://localhost:7771/api/v1/sessions/cs-a1b2c3
```

```json
{
  "id": "cs-a1b2c3",
  "mode": "libre",
  "status": "active",
  "messages": [
    {"role": "user",      "content": "Quelles colonnes...", "seq": 1},
    {"role": "assistant", "content": "Le fichier contient...", "seq": 2}
  ],
  "created_at": "2026-04-02T09:00:00Z"
}
```

---

## Fermer une session

```bash
curl -X DELETE http://localhost:7771/api/v1/sessions/cs-a1b2c3
```

Une session fermée passe en `status: closed`. Son historique reste en SQLite — vous pouvez le consulter, mais plus y envoyer de messages.

---

> **Référence complète :** [Briques-Chat](https://github.com/nidal-z/apollia-os/wiki/Briques-Chat) — les 7 endpoints REST, les 12 événements runtime, le schéma SQLite (`chat_sessions`, `chat_messages`, `chat_tool_authorizations`), l'injection mémoire utilisateur, et la résumisation automatique des longues conversations.
