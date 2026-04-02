# Tester et exécuter

Le code est complet. Cette section vous donne le fichier final à copier-coller, les étapes d'exécution, et quelques scénarios de test pour vérifier que tout fonctionne.

---

## Le fichier complet — copier-coller

Créez `file_assistant.py` :

```python
# file_assistant.py
import re
from datetime import datetime


def _now() -> str:
    return datetime.now().strftime("%Y-%m-%d %H:%M")


class FileAssistant:
    """Agent qui lit un fichier, le résume via LLM, et sauvegarde le résumé."""

    def manifest(self):
        return {
            "name": "file-assistant",
            "version": "1.0.0",
            "description": "Lit un fichier, le résume via LLM, sauvegarde le résumé",
            "tools_required": ["file_read", "file_write"],
            "max_concurrent_tasks": 1,
            "step_budget": 10,
        }

    def _extract_path(self, text: str) -> str | None:
        """Extrait le premier chemin de fichier trouvé dans le texte."""
        match = re.search(r'[~/.]?/[\w./-]+\.\w+', text)
        return match.group(0) if match else None

    async def run(self, task, ctx):
        # --- Extraire l'entrée ---
        parts = task["input"]["parts"]
        if not parts or parts[0].get("type") != "text":
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {"code": "MISSING_INPUT", "message": "Une entrée texte est requise"},
            }

        user_text = parts[0]["text"]

        # --- Extraire le chemin ---
        file_path = self._extract_path(user_text)
        if not file_path:
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "NO_FILE_PATH",
                    "message": f"Aucun chemin de fichier trouvé dans : {user_text!r}",
                },
            }

        # --- Lire le fichier ---
        read_result = await ctx.tools.call("file_read", {"path": file_path})
        if read_result.get("error"):
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "FILE_NOT_FOUND",
                    "message": f"Impossible de lire {file_path} : {read_result['error']}",
                },
            }

        file_content = read_result["content"]
        total_lines = read_result.get("total_lines", 0)

        # --- Résumer via LLM ---
        if ctx.llm is None:
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "LLM_UNAVAILABLE",
                    "message": "Aucun backend LLM configuré. Voir le chapitre 6.",
                },
            }

        response = await ctx.llm.chat(
            system=(
                "Tu es un assistant expert en synthèse de documents. "
                "Produis un résumé clair et concis en français. "
                "Le résumé doit tenir en 5 à 10 phrases. "
                "Commence directement par le résumé, sans introduction."
            ),
            user=f"Résume ce document ({total_lines} lignes) :\n\n{file_content}",
        )

        summary = response.content

        # --- Sauvegarder le résumé ---
        if "." in file_path.split("/")[-1]:
            base, ext = file_path.rsplit(".", 1)
            summary_path = f"{base}_summary.{ext}"
        else:
            summary_path = f"{file_path}_summary"

        await ctx.tools.call("file_write", {
            "path": summary_path,
            "content": f"Résumé de {file_path}\nGénéré le : {_now()}\n\n{summary}\n",
        })

        # --- Retourner le résultat ---
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [
                {
                    "type": "text",
                    "text": (
                        f"Résumé de {file_path} :\n\n{summary}\n\n"
                        f"Résumé sauvegardé dans : {summary_path}"
                    ),
                }
            ],
        }


agent = FileAssistant()
```

---

## Préparer un fichier de test

Créez un fichier texte à résumer :

```bash
cat > /tmp/rapport.txt << 'EOF'
Rapport trimestriel T3 2025 — Apollia Corp

Résultats financiers :
Les revenus du troisième trimestre s'élèvent à 2,4 millions d'euros, en
hausse de 18% par rapport au T3 2024 (2,03M€). Cette croissance est portée
principalement par le segment PME (+34%) et le renouvellement des contrats
Enterprise existants (+8%).

Les charges opérationnelles restent stables à 1,1M€ malgré l'accélération
de la croissance. La marge brute progresse de 3 points à 54%, confirmant
l'amélioration structurelle du modèle économique.

Perspectives :
Le pipeline commercial au 30 septembre représente 4,2M€ de revenus potentiels
sur les 12 prochains mois. La direction recommande d'accélérer le recrutement
commercial (3 postes ouverts) pour convertir ce pipeline avant fin 2025.

Le prochain rapport sera publié le 15 janvier 2026.
EOF
```

---

## Vérifier la configuration LLM

Avant de lancer l'agent, vérifiez qu'un backend LLM est configuré :

```bash
$ apollia-os status
  Runtime    ACTIVE
  LLM        anthropic (claude-3-5-haiku-20241022)   ← doit être présent
  Agents     0 actifs
```

Si `LLM` est absent ou affiche `(non configuré)`, consultez `~/.apollia/apollia.toml` et le chapitre 6.

Configuration minimale pour utiliser l'API Anthropic :

```toml
# ~/.apollia/apollia.toml
[llm]
default = "anthropic"

[[llm.backends]]
name = "anthropic"
type = "api"
provider = "anthropic"
model = "claude-3-5-haiku-20241022"
api_key_env = "ANTHROPIC_API_KEY"
```

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
apollia-os stop && apollia-os start   # redémarrer pour prendre en compte la config
```

---

## Exécution

```bash
# 1. Démarrer le runtime
$ apollia-os start
  ✔ Runtime prêt en 0.8s

# 2. Déployer l'agent
$ apollia-os agent start ./file_assistant.py
  Chargement de file_assistant.py...
  Validation AIP...
    ✔ manifest() — OK
    ✔ run() async — OK
    ✔ tools_required : file_read ✔  file_write ✔
  ✔ file-assistant [ACTIVE]

# 3. Envoyer une tâche
$ apollia-os run file-assistant "Résume /tmp/rapport.txt"
  -> Task t-xyz789 submitted to file-assistant
  Executing...
  Done in 2.1s (1 step, 3 tool calls)

  RESULT
  Résumé de /tmp/rapport.txt :

  Ce rapport présente les résultats du T3 2025 d'Apollia Corp. Les revenus
  atteignent 2,4M€, en hausse de 18% grâce au segment PME et au renouvellement
  des contrats Enterprise. Les charges opérationnelles restent stables à 1,1M€
  et la marge brute progresse à 54%. Le pipeline commercial représente 4,2M€
  de potentiel sur 12 mois. La direction recommande d'accélérer le recrutement
  commercial pour convertir ce pipeline avant fin 2025.

  Résumé sauvegardé dans : /tmp/rapport_summary.txt

# 4. Vérifier le fichier sauvegardé
$ cat /tmp/rapport_summary.txt
  Résumé de /tmp/rapport.txt
  Généré le : 2025-10-14 10:32

  Ce rapport présente les résultats du T3 2025 d'Apollia Corp. [...]
```

---

## Scénarios de test

### Test 1 — Fichier introuvable

```bash
$ apollia-os run file-assistant "Résume /tmp/inexistant.txt"

  RESULT (status: failed)
  CODE: FILE_NOT_FOUND
  MESSAGE: Impossible de lire /tmp/inexistant.txt : NOT_FOUND: fichier introuvable
```

### Test 2 — Aucun chemin dans l'entrée

```bash
$ apollia-os run file-assistant "Bonjour"

  RESULT (status: failed)
  CODE: NO_FILE_PATH
  MESSAGE: Aucun chemin de fichier trouvé dans : 'Bonjour'
```

### Test 3 — Formats d'instructions variés

L'extraction de chemin fonctionne avec différentes formulations :

```bash
$ apollia-os run file-assistant "Peux-tu résumer le contenu de /tmp/rapport.txt ?"
$ apollia-os run file-assistant "/tmp/rapport.txt — fais-en un résumé"
$ apollia-os run file-assistant "Que contient ./notes.md ?"
```

### Test 4 — Audit trail

```bash
$ apollia-os audit --last 5
  HEURE          AGENT            TÂCHE    OUTIL         DURÉE   RÉSULTAT
  10:32:18       file-assistant   t-xyz789 file_write    12ms    ✔
  10:32:16       file-assistant   t-xyz789 file_read     8ms     ✔
```

`3 tool calls` dans l'output incluent : 1 `file_read` + 1 appel LLM interne + 1 `file_write`. L'appel LLM (`ctx.llm.chat`) est comptabilisé dans le `step_budget` mais apparaît séparément dans les stats LLM, pas dans l'audit trail des outils.

---

## Ce que vous avez construit

En 60 lignes de Python :

- Un agent qui reçoit des instructions en langage naturel
- Lit un fichier de taille arbitraire
- Le résume intelligemment via LLM
- Sauvegarde le résultat de manière atomique
- Gère 4 cas d'erreur explicites
- Laisse une trace auditée de chaque opération

C'est un agent de production minimal mais complet. Les chapitres suivants vont enrichir ces concepts : les outils en profondeur (chapitre 4), la mémoire persistante pour retrouver les résumés passés (chapitre 5), les différents backends LLM (chapitre 6), et comment construire un agent beaucoup plus autonome avec la boucle ReAct (chapitre 6 également).
