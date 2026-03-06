"""Devis generator agent — generates commercial quotes.

Demonstrates ToolProxy (file_io) and MemoryInterface usage.
MVP: parsing is hardcoded, no LLM involved.
"""

import json


class DevisGenerator:
    """Generates a commercial quote from a text request."""

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
        user_input = task["input"]["parts"][0]["text"]

        devis = self._generate_devis(user_input)

        devis_json = json.dumps(devis, indent=2, ensure_ascii=False)
        await ctx.tools.call("file_io", {
            "action": "write",
            "path": f"devis/devis-{devis['numero']}.json",
            "content": devis_json,
        })

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
            "output": [
                {
                    "type": "text",
                    "text": f"Devis #{devis['numero']} genere : {devis['montant_ttc']} EUR TTC",
                }
            ],
        }

    def _generate_devis(self, user_input):
        return {
            "numero": "001",
            "client": "Dupont SA",
            "lignes": [{"description": "Conseil", "jours": 5, "tarif_jour": 850}],
            "montant_ht": 4250.0,
            "tva": 850.0,
            "montant_ttc": 5100.0,
        }


agent = DevisGenerator()
