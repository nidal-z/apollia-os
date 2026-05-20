# ResilienceLayer et circuit breakers

Un outil qui échoue répétitivement est l'un des scénarios les plus courants en production : un service externe tombe, un fichier temporairement verrouillé, un endpoint qui timeout. Sans protection, l'agent continue d'appeler l'outil en boucle jusqu'à épuiser son budget.

La **ResilienceLayer** d'Apollia OS résout ce problème avec un pattern éprouvé : le circuit breaker.

---

## La machine à états du circuit

Chaque outil enregistré dans le Tool Registry dispose de son propre circuit, indépendant des autres :

```
CLOSED (normal — les appels passent)
    │
    │ 3 échecs consécutifs
    ▼
OPEN (circuit coupé — retourne CircuitOpen immédiatement)
    │
    │ 30 secondes de cooldown écoulées
    ▼
HALF_OPEN (test — une seule tentative autorisée)
    │
    ├── succès → CLOSED (circuit restauré)
    └── échec  → OPEN   (cooldown réinitialisé)
```

### Paramètres par défaut

| Paramètre | Valeur |
|---|---|
| Seuil d'échec | 3 échecs consécutifs |
| Période de cooldown | 30 secondes |

Un succès unique en état CLOSED remet le compteur d'échecs à zéro.

---

## Comportement depuis l'agent

Quand un circuit est ouvert, l'appel à l'outil échoue immédiatement sans tentative d'exécution. L'erreur retournée est `CircuitOpen`. L'agent peut choisir de continuer avec d'autres outils ou de retourner une erreur propre à l'appelant :

```python
async def run(self, task, ctx):
    result = await ctx.tools.call("web_search", {"query": task["query"]})

    if result.error and result.error.code == "CIRCUIT_OPEN":
        # L'outil est temporairement indisponible — fallback
        ctx.log.warn("web_search_circuit_open",
                     fallback="using_cached_results")
        return AIPResult.completed(self._get_cached(task["query"]))

    return AIPResult.completed(result.output)
```

L'appel `CircuitOpen` consomme quand même 1 tool_call dans le StepBudget.

---

## RetryPolicy — backoff exponentiel avec jitter

Pour les erreurs transitoires (timeout réseau, service temporairement indisponible), l'ORIA Engine réessaie automatiquement avant de compter l'échec comme un vrai échec.

### Paramètres par défaut

| Paramètre | Valeur |
|---|---|
| `max_attempts` | 3 |
| `base_delay_ms` | 500 ms |
| `max_delay_ms` | 10 000 ms (10s) |
| `jitter` | ±25% |

### Séquence de retry

```
Tentative 1 : immédiate
Tentative 2 : ~500ms  (±25% jitter : entre 375ms et 625ms)
Tentative 3 : ~1000ms (±25% jitter : entre 750ms et 1250ms)
...
Maximum    : 10 000ms (cap)
```

Formule : `min(base_delay_ms × 2^(attempt-1), max_delay_ms)`, avec jitter de ±25%.

Le jitter évite les tempêtes de retry synchronisées quand plusieurs agents réessaient simultanément le même service.

### Classification des erreurs

Seules les erreurs `Transient` déclenchent un retry automatique :

| Classe | Retry | Exemples |
|---|---|---|
| `Transient` | Oui | Timeout réseau, service temporairement indisponible |
| `Permanent` | Non | Outil non trouvé, permissions refusées |
| `BudgetExceeded` | Non | StepBudget épuisé |
| `SandboxViolation` | Non | Tentative de sortie du sandbox |

Les erreurs `Permanent` échouent immédiatement — les retrier n'aurait aucun effet.

---

## Interaction RetryPolicy × StepBudget

Chaque tentative de retry consomme 1 tool_call dans le StepBudget :

```
Appel outil (Tentative 1) → 1 tool_call
  Échec Transient
Retry (Tentative 2, +500ms) → 1 tool_call supplémentaire
  Échec Transient
Retry (Tentative 3, +1000ms) → 1 tool_call supplémentaire
  Échec → 3 échecs consécutifs → circuit OPEN
```

Pour un agent qui fait beaucoup d'appels d'outils potentiellement instables, intégrez les retries dans le calcul du `max_tool_calls` :

```python
def manifest(self):
    return {
        "step_budget": {
            "max_tool_calls": 60   # 20 appels × 3 tentatives max
        }
    }
```

---

## Observabilité — logs et audit

L'état des circuits est visible dans les logs et l'audit :

```bash
# Voir les événements circuit en temps réel
RUST_LOG=apollia_oria=debug apollia-os start --foreground 2>&1 | grep "circuit"

# Statistiques agrégées
apollia-os audit stats
#  OUTIL            APPELS   SUCCÈS   TAUX
#  bash_executor    142      134      94.4%   [circuit: CLOSED]
#  web_search        89       71      79.8%   [circuit: CLOSED]
#  file_reader      203      203     100.0%   [circuit: CLOSED]
```

L'événement `ToolCircuitRestored` est émis sur l'EventBus quand un circuit passe de OPEN à CLOSED — utile pour les alertes de monitoring.
