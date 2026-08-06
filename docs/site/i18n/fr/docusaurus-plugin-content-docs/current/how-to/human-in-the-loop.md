---
sidebar_position: 10
title: Mettre un agent en pause pour une décision humaine
---

# Mettre un agent en pause pour une décision humaine

Parfois un agent ne doit pas décider seul : une action sensible exige un
feu vert, une règle manque, une valeur est incertaine. Apollia permet à
l'auteur de mettre en pause une tâche en cours d'exécution et d'exiger une
décision humaine avant qu'elle ne continue. Ce guide couvre la primitive
côté auteur, `NeedHumanInput`, et la façon dont la pause est résolue.

Ceci est le point de vue de l'auteur. Pour le point de vue de l'opérateur
qui approuve ou rejette depuis l'application desktop, voir l'aide
opérateur. Pour la façon dont les paliers d'autonomie évoluent quand une
approbation est requise, voir [Paliers d'autonomie](/explanation/autonomy-tiers).

## Lever `NeedHumanInput` pour mettre en pause

À l'intérieur d'un skill, levez `NeedHumanInput` quand une personne doit
trancher. Elle prend un `prompt` affiché à l'humain et un dict `context`
optionnel, persisté et restauré autour de la pause.

```python
from apollia import agent, skill, NeedHumanInput
from apollia.types import Ctx


@agent(name="invoice-router", version="0.1.0", description="Route invoices.")
class InvoiceRouter:
    @skill("invoice.route", description="Decide where to file an invoice.")
    async def route(self, vendor: str, amount: float, ctx: Ctx) -> dict:
        folder = await self._lookup_rule(vendor, ctx)
        if folder is None:
            raise NeedHumanInput(
                prompt=f"No rule for {vendor} ({amount:.2f}). Approve filing under 'to-review'?",
                context={"vendor": vendor, "amount": amount},
            )
        return {"folder": folder}
```

Le constructeur est `NeedHumanInput(prompt: str, context: dict | None = None)`.
C'est une sous-classe d'`AgentError`, importée depuis la racine du paquet
(`from apollia import NeedHumanInput`).

## Ce que la pause déclenche

Quand un skill lève `NeedHumanInput`, le dispatcher la transforme en un
résultat de statut `input_required` portant le `prompt` et le `context`. Le
runtime suspend la tâche, persiste son état, et la fait remonter à
l'opérateur. La tâche attend : une minute ou une semaine, l'état reste en
l'état jusqu'à ce qu'un humain réponde.

Écrivez un prompt clair. Sa qualité conditionne la qualité de la décision.

- Faible : `"Continue?"`
- Meilleur : `"No rule for 'Acme Corp' (1240.00). Approve filing under 'to-review'?"`

Gardez `context` exempt de secrets et de données personnelles superflues.
Il est sérialisé, stocké, et affiché dans l'interface.

## Résoudre la pause

Un opérateur voit les tâches en attente et y répond. Depuis le CLI :

```sh
# Lister les tâches en attente d'une décision humaine
apollia-os task list --pending-approval

# Approuver, ou rejeter avec une raison
apollia-os task resume <task-id> --approve
apollia-os task resume <task-id> --reject --reason "file it manually this quarter"

# Consulter les décisions déjà résolues
apollia-os task approvals
```

Rejeter met fin à la tâche avec un statut rejeté. Approuver laisse
l'exécution continuer. La décision que l'humain renvoie est un booléen plus
une raison optionnelle sous forme de chaîne ; ce n'est pas une réponse
libre ni une valeur choisie.

## Ce que votre skill reçoit à la reprise

Soyez précis sur ce contrat, pour ne pas construire sur quelque chose qui
n'existe pas. La décision humaine est appliquée par le runtime : un rejet
termine la tâche, une approbation la laisse se poursuivre. Un `@skill`
classique ne reçoit ni la décision ni la raison en argument quand la
tâche reprend, donc n'essayez pas de relire la réponse depuis l'intérieur
du skill. En particulier, il n'existe aucune clé `ctx.memory` ou
`ctx.profile` que le runtime remplirait avec la réponse humaine ; lire
une telle clé n'est pas un mécanisme pris en charge.

Concevez en conséquence :

- Utilisez `NeedHumanInput` comme une barrière, "ne pas continuer sans
  approbation humaine", plutôt que comme un canal pour collecter une
  donnée venant de l'humain.
- Rendez idempotente la condition qui déclenche la pause, pour que la
  tâche se comporte correctement si le même skill est réentré.
- Le branchement conscient de la reprise après une suspension est géré
  par le runtime pour le chat et les exécutions orchestrées ; un skill de
  worker autonome n'observe pas lui-même la reprise.

## `NeedHumanInput` face à `requires_approval`

Pour le cas courant où l'on verrouille une action sensible précise,
préférez la forme déclarative : marquez le skill avec
`@skill(..., requires_approval=True)`. Le runtime insère la pause
d'approbation avant l'exécution du skill, sans que votre code n'ait
besoin de lever quoi que ce soit. Réservez `NeedHumanInput` au cas où la
décision de mettre en pause est dynamique et prise en cours de skill.

Un troisième mécanisme, distinct, verrouille les outils MCP externes par
serveur ou par outil ; il se configure via les commandes `apollia-os mcp`,
pas via le SDK agent.

## Voir aussi

- [Paliers d'autonomie](/explanation/autonomy-tiers) pour la façon dont les
  approbations requises s'articulent avec le modèle d'autonomie.
- Les commandes `task` dans la [référence CLI](/reference/cli).
- Le [contrat SDK / ctx](/reference/sdk) pour les surfaces qu'utilise un skill.
