# [SPRINT-6][agents] Agent devis-generator complet

**ID :** STORY-044
**Sprint :** 6
**Crate cible :** `agents/` (Python) + `apollia-runtime` (DT-031 fix)
**Fichier(s) cible(s) :** `agents/hello_agent.py`, `agents/devis_agent.py`, `crates/apollia-runtime/src/api/routes_agents.rs`
**Taille :** L
**Depend de :** STORY-041 (ResilienceLayer), STORY-042 (RetryPolicy), Sprint 5 (CLI + API)
**Statut :** 🔲 A faire

---

## User Story

```
En tant qu'operateur PME,
je veux deployer un agent devis-generator et lui soumettre une tache via la CLI,
afin de generer un devis commercial de bout en bout sans ecrire de code.
```

---

## Contexte technique

C'est la premiere story qui exerce la chaine complete : CLI → Supervisor → TaskRouter → Coordinator → ORIA → AIPBridge → Python agent. Elle valide que toutes les briques des Sprints 0-5 fonctionnent ensemble en conditions reelles.

**Prerequis :** DT-031 doit etre resolu — `manifest_from_path()` dans routes_agents.rs doit charger le module Python via `AIPLoader` au lieu de retourner un manifest placeholder.

L'agent devis-generator utilise le Mode Direct (tache atomique, < 4 outils). Il demonstre l'utilisation de `ToolProxy` (file_io pour lire/ecrire des fichiers), `MemoryInterface` (stocker les devis generes), et le RuntimeContext complet.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #3 — Contrat minimal (duck typing Python, manifest() + run())
- Principe #1 — Local-first (tout tourne localement, zero cloud)
- Principe #6 — Memoire a initiative de l'agent (l'agent decide quoi memoriser)

**Position dans l'architecture :**
```
apollia-os run devis-generator "..."
  └── CLI → API → TaskRouter → Coordinator
        └── ORIA (Mode Direct)
              └── AIPBridge → devis_agent.py  <-- cette story
                    ├── ctx.tools.file_io
                    └── ctx.memory.record/remember
```

---

## Criteres d'Acceptation

### AC-1 — hello_agent.py minimal AIP-compatible

```
ETANT DONNE agents/hello_agent.py avec manifest() et async run()
QUAND apollia-os agent start agents/hello_agent.py est execute
ALORS l'agent est charge via AIPLoader, valide, et enregistre dans AgentRegistry en etat ACTIVE
```

### AC-2 — hello_agent execute une tache simple

```
ETANT DONNE hello_agent demarre et ACTIVE
QUAND apollia-os run hello-agent "Bonjour" est execute
ALORS AIPResult(status=completed, output=["Bonjour ! Je suis hello-agent."]) est retourne
ET la tache est tracee dans l'audit log
```

### AC-3 — devis_agent.py utilise ToolProxy et MemoryInterface

```
ETANT DONNE devis_agent demarre avec tools_required=["file_io"] et memory_namespace="devis"
QUAND apollia-os run devis-generator "Devis pour Dupont SA, 5 jours, 850EUR/jour" est execute
ALORS l'agent utilise ctx.tools pour lire/ecrire des fichiers
ET l'agent utilise ctx.memory pour stocker le devis
ET AIPResult(status=completed) est retourne avec le devis en output
```

### AC-4 — DT-031 resolu : manifest_from_path charge le module Python reel

```
ETANT DONNE un fichier agent Python valide
QUAND POST /api/v1/agents avec le path est envoye
ALORS AIPLoader charge le module Python
ET le manifest reel de l'agent est utilise (pas un placeholder)
```

### AC-5 — Agent avec outil manquant passe en DEGRADED

```
ETANT DONNE un agent avec tools_optional=["mcp_erp"] (outil non enregistre)
QUAND l'agent est demarre
ALORS ProcessState passe a DEGRADED (pas d'erreur, l'agent fonctionne)
ET un warning est emis sur l'EventBus
```

### AC-6 — Erreur de chargement Python retourne une erreur claire

```
ETANT DONNE un fichier Python invalide (syntaxe erreur, pas de manifest())
QUAND apollia-os agent start agents/broken.py est execute
ALORS une erreur claire est affichee avec le detail Python
ET l'agent n'est PAS enregistre dans le registry
```

---

## Specification technique

### Fichiers a creer

**agents/hello_agent.py :**
```python
class HelloAgent:
    def manifest(self):
        return {
            "name": "hello-agent",
            "version": "1.0.0",
            "description": "Agent de demonstration minimal",
            "tools_required": [],
            "max_concurrent_tasks": 1,
        }

    async def run(self, task, ctx):
        text = task["input"]["parts"][0]["text"] if task.get("input", {}).get("parts") else "monde"
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"Bonjour ! J'ai recu : {text}"}],
        }

agent = HelloAgent()
```

**agents/devis_agent.py :**
```python
import json

class DevisGenerator:
    def manifest(self):
        return {
            "name": "devis-generator",
            "version": "1.0.0",
            "description": "Genere des devis commerciaux",
            "tools_required": ["file_io"],
            "memory_namespace": "devis",
            "max_concurrent_tasks": 1,
        }

    async def run(self, task, ctx):
        # 1. Parser la demande
        user_input = task["input"]["parts"][0]["text"]

        # 2. Generer le devis (logique simplifiee MVP)
        devis = self._generate_devis(user_input)

        # 3. Sauvegarder via file_io
        devis_json = json.dumps(devis, indent=2, ensure_ascii=False)
        await ctx.tools.call("file_io", {
            "action": "write",
            "path": f"devis/devis-{devis['numero']}.json",
            "content": devis_json,
        })

        # 4. Memoriser le devis
        if ctx.memory:
            await ctx.memory.record(
                f"Devis #{devis['numero']} genere pour {devis['client']}",
                importance=0.8,
            )
            await ctx.memory.remember(
                f"client.{devis['client'].lower().replace(' ', '_')}.dernier_devis",
                devis,
            )

        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"Devis #{devis['numero']} genere : {devis['montant_ttc']} EUR TTC"}],
        }

    def _generate_devis(self, user_input):
        # MVP : parsing simplifie sans LLM
        return {
            "numero": "001",
            "client": "Dupont SA",
            "lignes": [{"description": "Conseil", "jours": 5, "tarif_jour": 850}],
            "montant_ht": 4250.0,
            "tva": 850.0,
            "montant_ttc": 5100.0,
        }

agent = DevisGenerator()
```

### Modifications a faire

**DT-031 — `routes_agents.rs` :** Remplacer `manifest_from_path()` placeholder par un appel reel a `AIPLoader::load_agent_module()` + `validate_agent()`. Necessite d'ajouter une dependance `apollia-aip` a `apollia-runtime` ou de passer le `ValidatedAgent` via l'API.

### Dependances Cargo

```toml
# Potentiellement dans apollia-runtime/Cargo.toml si DT-031 est resolu inline
apollia-aip = { path = "../apollia-aip" }
```

### Ce que cette story N'implemente PAS

- L'utilisation d'un LLM pour le parsing de la demande — MVP hardcode la logique de devis
- Le Mode Orchestre pour l'agent — l'agent tourne en Mode Direct
- La gestion multi-devises ou multi-langues
- Les templates de devis personnalisables
- Le deploiement de l'agent via un package manager

---

## Tests requis

### Tests unitaires (logique Python)

Tests manuels via `apollia-os run` — la validation unitaire de l'agent Python est hors scope Rust.

### Tests d'integration

```rust
// Dans crates/apollia-runtime/tests/ ou STORY-045
// Test que AIPLoader charge hello_agent.py sans erreur
// Test que ValidatedAgent a le bon manifest
// Test que manifest_from_path retourne le vrai manifest
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test --workspace` passe
- [ ] `cargo clippy --workspace -- -D warnings` : zero warning
- [ ] DT-031 resolu (manifest_from_path charge le module Python reel)
- [ ] `agents/hello_agent.py` est AIP-compatible (manifest + run)
- [ ] `agents/devis_agent.py` utilise ToolProxy et MemoryInterface

**Fonctionnel :**
- [ ] `apollia-os agent start agents/hello_agent.py` reussit
- [ ] `apollia-os run hello-agent "Bonjour"` retourne un resultat
- [ ] `apollia-os run devis-generator "Devis Dupont"` genere un devis
- [ ] La chaine complete CLI → API → ORIA → Python fonctionne

**Commit :**
- [ ] `feat(agents): add hello_agent and devis_agent demo agents`
- [ ] `fix(apollia-runtime): resolve DT-031 manifest_from_path loads real Python module`

---

## Liens

- DT-031 : manifest_from_path MVP dans routes_agents.rs
- Story precedente : STORY-042 (RetryPolicy)
- Story suivante : STORY-045 (Tests e2e)
- Spec : `docs/Architecture-Vue-Ensemble.md` (section AIP agent minimal)
