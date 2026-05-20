# `NeedHumanInput`

Quand l'agent rencontre une situation où il ne peut pas trancher seul (validation d'une action sensible, choix entre plusieurs options, confirmation d'une donnée incertaine), il lève `NeedHumanInput`. Le boundary trappe et produit un `AIPResult` avec `status="input_required"`. Le runtime **suspend la tâche**, persiste l'état, et la **reprend automatiquement** quand l'humain a répondu.

C'est le mécanisme HITL (Human-in-the-Loop) du SDK. Pas de coordination manuelle à écrire.

---

## Pattern

```python
from apollia import agent, skill, NeedHumanInput
from apollia.types import Ctx


@agent(name="invoice-router", version="0.1.0", description="Route invoices.")
class InvoiceRouter:
    @skill("invoice.route", description="Decide where to file an invoice.")
    async def route(self, vendor: str, amount: float, ctx: Ctx) -> dict:
        match = await self._find_rule(vendor, ctx)
        if match is None:
            raise NeedHumanInput(
                prompt=f"Aucune règle pour {vendor} ({amount:.2f}€). Où ranger cette facture ?",
                context={
                    "vendor": vendor,
                    "amount": amount,
                    "candidates": ["frais-bureau", "logiciels", "freelance"],
                },
            )
        return {"folder": match.folder, "rule_id": match.id}
```

Deux arguments : `prompt` (la question à l'humain) et `context` (un dict que l'humain et le runtime voient pendant la pause, restitué intact à la reprise).

---

## Le contrat

```python
class NeedHumanInput(AgentError):
    def __init__(
        self,
        prompt: str,
        context: dict[str, Any] | None = None,
    ) -> None: ...
```

### `prompt`

Phrase claire affichée à l'humain dans l'UI Desktop ou la CLI interactive. Sa qualité conditionne la qualité de la réponse. Soyez précis :

- Mauvais : `"Continuer ?"`
- Correct : `"Aucune règle pour 'Acme Corp' (1240€). Où ranger cette facture ?"`
- Très bien : `"Facture Acme Corp à 1240€ HT. Trois rangements possibles : frais-bureau, logiciels, freelance. Lequel ?"`

### `context`

Tout ce que l'humain doit voir, plus tout ce que l'agent voudra récupérer à la reprise. Le runtime persiste verbatim et le restitue dans `ctx` au retour.

---

## Cycle de vie d'une tâche suspendue

1. **Suspension :** l'agent lève `NeedHumanInput`. Le boundary produit `AIPResult.input_required(prompt, context)`. Le runtime persiste l'état complet de la tâche (étape, payload, context).
2. **Notification :** l'UI Desktop affiche le prompt et le context. La CLI interactive le propose. Si l'agent a configuré `ctx.notify`, une notification est envoyée.
3. **Réponse :** l'humain saisit sa décision dans l'UI (texte libre, choix multiple, validation).
4. **Reprise :** le runtime ré-invoque la même skill avec le payload original, **plus** le context restauré et la réponse humaine accessible.

L'agent ne sait pas (et n'a pas à savoir) combien de temps la pause a duré. Une minute, un jour, une semaine : tant que l'humain n'a pas répondu, le state reste persistant.

---

## Lire la réponse au retour

À la reprise, la réponse humaine est accessible via `ctx`. Le pattern recommandé : structurer le `context` pour distinguer les états « avant pause » et « après réponse ».

```python
@skill("invoice.route", description="…")
async def route(self, vendor: str, amount: float, ctx: Ctx) -> dict:
    # Détecter si on est dans la reprise
    last_response = await ctx.memory.recall(f"hitl.{vendor}.response")
    if last_response:
        # Reprise : appliquer le choix humain
        return {"folder": last_response, "rule_id": "human_decision"}

    # Première passe : demander
    match = await self._find_rule(vendor, ctx)
    if match is None:
        raise NeedHumanInput(
            prompt=f"Où ranger la facture {vendor} ({amount:.2f}€) ?",
            context={"vendor": vendor, "amount": amount},
        )
    return {"folder": match.folder, "rule_id": match.id}
```

Variante plus directe avec un `ctx.profile` pour stocker la décision finale :

```python
folder = await ctx.profile.get(f"invoice.routing.{vendor}")
if folder is None:
    raise NeedHumanInput(...)
return {"folder": folder}
```

> **Référence technique :** la mécanique exacte de persistance (table `task_state`, format `is_resumed`, restitution `input_response`) sera dans la page wiki `Briques-HITL-Engine` *(wiki disponible prochainement)*.

---

## HITL vs `requires_approval`

Deux mécanismes complémentaires :

| Cas | Mécanisme |
|---|---|
| L'agent a besoin d'une **donnée** ou d'un **choix** humain pour continuer | `raise NeedHumanInput(...)` |
| Une **tool call** est sensible et doit être validée avant exécution | `@skill(..., requires_approval=True)` (cf. [chapitre 7](../part-ii-the-decorators/07-skill-decorator.md)) |

`requires_approval` est déclaratif : le runtime intercale automatiquement la pause avant l'exécution du tool, sans que l'agent ne lève d'exception. `NeedHumanInput` est explicite : l'agent décide à un moment précis qu'il a besoin d'une réponse.

---

## Notifications

`NeedHumanInput` ne déclenche **pas** automatiquement une notification. Si vous voulez alerter l'utilisateur d'une pause urgente :

```python
await ctx.notify.publish(
    f"Action requise : facture {vendor} à valider.",
    severity="warning",
    title="Invoice triage",
)
raise NeedHumanInput(prompt=..., context=...)
```

L'UI Desktop affiche la notification, l'utilisateur clique, arrive sur le prompt.

---

## Anti-patterns

**Ne pas** utiliser `NeedHumanInput` pour signaler une erreur. C'est `DomainError` qui sert à ça. `NeedHumanInput` veut dire : « j'ai besoin d'une décision humaine pour continuer », pas « j'ai échoué ».

**Ne pas** boucler sur `NeedHumanInput` avec `input()` ou un prompt console. Le mécanisme est asynchrone : vous levez, le runtime suspend, l'humain répond plus tard, la tâche reprend. Tout `input()` synchrone bloque le worker.

**Ne pas** mettre des données sensibles dans `context`. Le context est sérialisé, persisté en DB, et affiché dans l'UI. Pas de secrets, pas de PII non nécessaire.

**Ne pas** lever `NeedHumanInput` depuis un `@orchestrated`. Le moteur ORIA gère sa propre approbation HITL via `tools_requiring_approval`. `NeedHumanInput` est conçu pour les `@skill` et `@on_message`.

---

## ADRs

- `ADR-023` : HITL `is_resumed`, `input_response`, `tools_requiring_approval`
- `ADR-100` : Exceptions typées au boundary
- `ADR-109` : AIPResult interne au SDK

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
