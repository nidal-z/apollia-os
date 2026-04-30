# Setup — email-triage

⚠️ **Cet agent assume des wrappers Gmail API via `http_fetch`. Apollia v0.1 ne fournit pas d'outils gmail natifs.** L'agent fonctionne mais demande une étape supplémentaire d'auth Gmail.

## Prérequis
- Apollia OS v0.1.0+
- Backend LLM configuré (ORIA Reasoner consomme du LLM)
- Compte Google avec API Gmail activée
- OAuth2 credentials Google : `client_id`, `client_secret`, `refresh_token`

## Setup credentials Gmail

### 1. Créer un projet Google Cloud
- Aller sur https://console.cloud.google.com
- Créer un projet, activer l'API Gmail
- Créer des credentials OAuth2 type "Desktop app"

### 2. Obtenir refresh_token
Procédure manuelle (à automatiser dans une story future) :
```bash
# scope minimal : gmail.modify (lit + label + draft, pas d'envoi sans HITL)
# Suivre le flow OAuth2 Google et récupérer le refresh_token
```

### 3. Stocker en mémoire
```bash
apollia memory set gmail.client_id "<client-id>" --namespace email-triage
apollia memory set gmail.client_secret "<client-secret>" --namespace email-triage
apollia memory set gmail.refresh_token "<refresh-token>" --namespace email-triage
```

⚠️ Ces tokens sont sensibles. Vérifier que la base mémoire SQLite est sur disque chiffré.

## Configuration APOLLIA.md

3 sections obligatoires (voir APOLLIA.md du package) :
- `## Email Triage — Classification Rules` : tes règles de classement
- `## Email Triage — VIP List` : tes expéditeurs prioritaires
- `## Email Triage — Auto-Reply Templates` : tes templates de réponse

Sans ces sections, l'agent classera de façon générique (qualité dégradée).

## HITL — Comportement attendu

L'agent a `tools_requiring_approval = ["http_fetch"]`. Conséquence : **avant chaque appel Gmail API**, le runtime suspend et te demande d'approuver. Cela inclut :
- ✅ Lecture inbox : approbation oui (read-only)
- ⚠️ Marquage label : approbation
- ❌ Envoi email : approbation **strictement obligatoire**

Si tu trouves le HITL trop bavard pour les lectures, tu peux exclure `http_fetch` de `tools_requiring_approval` ET wrapper séparément l'envoi (custom tool `gmail_send`). Cette amélioration est laissée comme story candidate.

## Premier run

```bash
apollia agent run email-triage --input "Triage les 10 derniers non lus"
```

L'ORIA Reasoner va générer un plan multi-step. Tu verras l'enchaînement et chaque step `http_fetch` te demandera approbation.

## Profil utilisateur

L'agent **ne lit pas dynamiquement** `user.agents.hitl` (gap connu Apollia v0.1). Pour personnaliser le comportement HITL :
- `never` : retire `http_fetch` de `tools_requiring_approval` dans le manifest (NON recommandé)
- `always` : laisse tel quel (défaut)
- `approval` : conserve `http_fetch` mais ajoute logique custom dans on_plan_complete (story candidate)

## Limitations

| Limitation | Impact | Workaround |
|---|---|---|
| Pas d'outil gmail natif Apollia | http_fetch + auth manuelle | Cette config setup |
| `user.agents.hitl` non lu dynamiquement | Comportement HITL fixé au manifest | Manifest à éditer manuellement |
| Pas de fan-out parallèle dans ORIA | Triage séquentiel | Acceptable pour <100 emails |
| Pas de support PJ | Triage sur métadonnées + body texte | Wrapper PJ via tool custom |

## Roadmap (stories candidates)

1. Plugin gmail natif Apollia (lit, label, draft, send distincts) → résout le gros du HITL bavard
2. Lecture dynamique `user.agents.hitl` au runtime (modifier `tools_requiring_approval` à chaud) → personnalisation profil
3. Support pièces jointes
