# Les Worker Agents

Trois Worker Agents, trois domaines d'expertise. Chaque fichier est complet et autonome — copiez-les dans `agents/` et ils fonctionnent sans modification.

---

## pdf-invoice-worker.py

```python
"""pdf-invoice-worker — extraction de données structurées depuis des factures PDF fournisseurs.

Extrait le numéro, la date, le fournisseur, le SIRET et les montants HT/TVA/TTC.
Retourne un objet JSON normalisé, indépendant de la mise en page du PDF.
"""
from __future__ import annotations
from typing import Any
import json

SYSTEM_PROMPT: str = """Tu es un spécialiste de l'extraction de données de factures PDF.

## RÈGLES ABSOLUES

1. TOUJOURS utiliser pdfplumber (pas PyPDF2) pour l'extraction.
   RAISON : PyPDF2 ignore les espaces entre colonnes — les montants sont concaténés silencieusement.

2. TOUJOURS extraire le texte page par page avec `page.extract_text()`, jamais en une seule passe.
   RAISON : Les factures multi-pages mélangent les données si on concatène sans séparateur.

3. JAMAIS inférer un montant manquant. Si TTC, HT ou TVA est absent, retourner null pour ce champ.
   RAISON : Une valeur devinée génère une écriture comptable fausse sans alerte.

4. TOUJOURS valider que montant_ht + tva ≈ montant_ttc (tolérance 0.02 €).
   RAISON : Un PDF mal formaté peut contenir des montants incohérents — l'alerter tôt évite une validation silencieuse.

5. JAMAIS modifier le fichier source — lecture seule.
   RAISON : La facture originale est une pièce comptable légale.

## IMPORTS STANDARDS

```python
import pdfplumber
import re
import json
```

## PATTERNS OBLIGATOIRES

```python
# Ouverture sécurisée
with pdfplumber.open(file_path) as pdf:
    pages_text = []
    for page in pdf.pages:
        text = page.extract_text or ""
        pages_text.append(text)
    full_text = "\n--- PAGE ---\n".join(pages_text)

# Extraction de montant (formats : 1 234,56 € ou 1234.56 EUR)
def extract_amount(text: str, label: str) -> float | None:
    pattern = rf"{label}[\\s:]*([\\d\\s]+[,\\.][\\d{{2}}])\\s*(?:€|EUR)"
    match = re.search(pattern, text, re.IGNORECASE)
    if not match:
        return None
    return float(match.group(1).replace(" ", "").replace(",", "."))

# Sérialisation de résultat
return AIPResult.completed(json.dumps(result, ensure_ascii=False))
```

## GESTION DES ERREURS DOMAINE

- `FileNotFoundError`   → failed("fichier_introuvable", "PDF introuvable : {path}")
- `pdfplumber.PDFSyntaxError` → failed("pdf_corrompu", "PDF illisible ou corrompu")
- Page vide (text == "") → failed("pdf_vide", "Aucun texte extractible — PDF scanné non-OCR ?")
- Montants incohérents  → failed("montants_incoherents", "HT + TVA ≠ TTC (écart > 0.02 €)")
"""


def manifest() -> dict[str, Any]:
    return {
        "name": "pdf-invoice-worker",
        "version": "1.0.0",
        "description": (
            "Extrait le numéro, la date, le fournisseur, le SIRET "
            "et les montants HT/TVA/TTC d'une facture PDF fournisseur."
        ),
        "execution_mode": "direct",
        "tools_required": ["file_io", "python_executor"],
        "tools_optional": [],
        "packages": ["pdfplumber"],
        "memory_namespace": "pdf-invoice-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "extract-invoice",
                "name": "Extraire une facture PDF",
                "description": (
                    "Extrait les données structurées d'une facture fournisseur PDF : "
                    "numéro de facture, date, nom fournisseur, SIRET, montants HT/TVA/TTC. "
                    "Retourne un objet JSON normalisé."
                ),
                "input_modes": ["text"],
                "output_modes": ["json"],
                "input_schema": {
                    "file_path": {
                        "type": "string",
                        "description": "Chemin absolu vers le fichier PDF",
                        "required": True,
                    }
                },
            }
        ],
        "tags": ["pdf", "facture", "extraction", "comptabilite"],
        "dangerous_tools_allowed": False,
    }


class PdfInvoiceWorker:
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 6
    TEMPERATURE = 0.0

    async def run(self, task, ctx):
        from pathlib import Path

        parts = task["input"]["parts"]
        message = next((p["text"] for p in parts if p.get("type") == "text"), "")

        # Guardrail 1 : extraire le chemin de fichier
        file_path = self._extract_path(message)
        if not file_path:
            return AIPResult.failed(
                "chemin_manquant",
                "Aucun chemin de fichier PDF trouvé dans la requête.",
            )

        path = Path(file_path)

        # Guardrail 2 : existence et extension
        if not path.exists():
            return AIPResult.failed(
                "fichier_introuvable",
                f"Fichier introuvable : {file_path}",
            )
        if path.suffix.lower() != ".pdf":
            return AIPResult.failed(
                "format_non_supporte",
                f"Extension non supportée : {path.suffix} (attendu : .pdf)",
            )

        # Guardrail 3 : taille raisonnable (< 50 Mo)
        if path.stat().st_size > 50 * 1024 * 1024:
            return AIPResult.failed(
                "fichier_trop_grand",
                "Fichier PDF supérieur à 50 Mo — traitement impossible.",
            )

        try:
            result = await self.react(task, ctx, message)
            return result
        except Exception as exc:
            error_map = {
                "FileNotFoundError": ("fichier_introuvable", f"PDF introuvable : {file_path}"),
                "PDFSyntaxError":    ("pdf_corrompu",        "PDF illisible ou corrompu"),
                "MemoryError":       ("fichier_trop_grand",  "PDF trop volumineux pour l'extraction"),
            }
            code, msg = error_map.get(type(exc).__name__, ("erreur_interne", str(exc)))
            return AIPResult.failed(code, msg)

    def _extract_path(self, message: str) -> str | None:
        import re
        match = re.search(r"(/[^\s]+\.pdf)", message, re.IGNORECASE)
        return match.group(1) if match else None


agent = PdfInvoiceWorker()
```

---

## invoice-validator-worker.py

```python
"""invoice-validator-worker — validation métier des données extraites d'une facture fournisseur.

Vérifie la cohérence des montants, la validité de la date, la présence des champs obligatoires,
et signale les anomalies (doublons, montants hors plafond, TVA incorrecte).
"""
from __future__ import annotations
from typing import Any
import json

SYSTEM_PROMPT: str = """Tu es un expert en validation de factures fournisseurs.

## RÈGLES ABSOLUES

1. TOUJOURS vérifier que montant_ht + tva ≈ montant_ttc (tolérance 0.02 €).
   RAISON : Une incohérence non signalée = écriture comptable incorrecte.

2. TOUJOURS vérifier que le taux de TVA est standard (0%, 5.5%, 10%, 20%).
   RAISON : Un taux non-standard indique une erreur de saisie ou une fraude.

3. JAMAIS rejeter une facture pour un champ optionnel manquant (SIRET, numéro de commande).
   RAISON : Certains fournisseurs étrangers n'ont pas de SIRET — ce n'est pas une erreur bloquante.

4. TOUJOURS retourner le JSON de la facture originale dans le champ "facture" de la réponse.
   RAISON : Le step suivant (compta-worker) a besoin des données pour générer l'écriture.

5. Si montant_ttc > 5000 €, TOUJOURS inclure "alerte_montant": true dans la réponse.
   RAISON : Les factures de montant élevé requièrent une approbation humaine avant comptabilisation.

## PATTERNS OBLIGATOIRES

```python
import json

# Parser l'input JSON
data = json.loads(input_text)
anomalies = []

# Vérification TVA
taux_tva_standard = [0.0, 5.5, 10.0, 20.0]
if data.get("montant_ht") and data.get("tva"):
    taux = round(data["tva"] / data["montant_ht"] * 100, 1)
    if taux not in taux_tva_standard:
        anomalies.append(f"Taux TVA non standard : {taux}%")

# Réponse normalisée
result = {
    "statut": "VALIDE" if not anomalies else "ANOMALIE",
    "facture": data,
    "anomalies": anomalies,
    "alerte_montant": (data.get("montant_ttc", 0) or 0) > 5000,
}
return AIPResult.completed(json.dumps(result, ensure_ascii=False))
```

## GESTION DES ERREURS DOMAINE

- Input non-JSON              → failed("input_invalide", "Les données d'extraction ne sont pas du JSON valide")
- Champs obligatoires absents → failed("champs_manquants", "Champs requis manquants : {champs}")
- Montants négatifs           → failed("montants_negatifs", "Les montants ne peuvent pas être négatifs")
"""


def manifest() -> dict[str, Any]:
    return {
        "name": "invoice-validator-worker",
        "version": "1.0.0",
        "description": (
            "Valide la cohérence des données d'une facture fournisseur : "
            "montants HT/TVA/TTC, taux TVA standard, champs obligatoires présents. "
            "Signale les anomalies et les factures à montant élevé (> 5 000 €)."
        ),
        "execution_mode": "direct",
        "tools_required": ["python_executor"],
        "tools_optional": [],
        "packages": [],
        "memory_namespace": "invoice-validator-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "validate-invoice",
                "name": "Valider une facture",
                "description": (
                    "Valide la cohérence métier d'une facture fournisseur (JSON normalisé) : "
                    "vérification TVA, montants cohérents, champs obligatoires. "
                    "Retourne statut VALIDE ou ANOMALIE avec la liste des anomalies détectées."
                ),
                "input_modes": ["json"],
                "output_modes": ["json"],
            }
        ],
        "tags": ["validation", "facture", "comptabilite", "tva"],
        "dangerous_tools_allowed": False,
    }


class InvoiceValidatorWorker:
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 4
    TEMPERATURE = 0.0

    async def run(self, task, ctx):
        import json as _json

        parts = task["input"]["parts"]
        input_text = next((p["text"] for p in parts if p.get("type") == "text"), "")

        # Guardrail 1 : input doit être du JSON valide
        try:
            data = _json.loads(input_text)
        except (_json.JSONDecodeError, ValueError):
            return AIPResult.failed(
                "input_invalide",
                "Les données reçues ne sont pas du JSON valide.",
            )

        # Guardrail 2 : champs obligatoires
        champs_requis = ["numero", "date", "fournisseur", "montant_ht", "tva", "montant_ttc"]
        manquants = [c for c in champs_requis if data.get(c) is None]
        if manquants:
            return AIPResult.failed(
                "champs_manquants",
                f"Champs requis manquants : {', '.join(manquants)}",
            )

        # Guardrail 3 : montants non négatifs
        for champ in ["montant_ht", "tva", "montant_ttc"]:
            if data.get(champ, 0) < 0:
                return AIPResult.failed(
                    "montants_negatifs",
                    f"Le champ '{champ}' est négatif ({data[champ]}).",
                )

        try:
            result = await self.react(task, ctx, input_text)
            return result
        except Exception as exc:
            return AIPResult.failed("erreur_interne", str(exc))


agent = InvoiceValidatorWorker()
```

---

## compta-worker.py

```python
"""compta-worker — génération d'écritures comptables à partir de factures validées.

Génère une écriture au format débit/crédit et l'ajoute au fichier CSV d'export comptable.
JAMAIS de doublon : vérification par numéro de facture avant insertion.
"""
from __future__ import annotations
from typing import Any
import json

SYSTEM_PROMPT: str = """Tu es un expert en comptabilité générale (plan comptable français).

## RÈGLES ABSOLUES

1. JAMAIS insérer une écriture si le numéro de facture existe déjà dans le CSV d'export.
   RAISON : Un doublon comptable est une erreur légale difficile à corriger a posteriori.

2. TOUJOURS générer deux lignes : débit (compte 401 Fournisseurs) et crédit (compte 44566 TVA déductible + 60x Charges).
   RAISON : Le principe de la partie double est non-négociable en comptabilité.

3. TOUJOURS utiliser le format CSV avec ces colonnes exactes (dans cet ordre) :
   ecriture_id, date, numero_facture, fournisseur, compte_debit, compte_credit, montant, libelle
   RAISON : Le format est importé directement dans le logiciel comptable — toute déviation bloque l'import.

4. JAMAIS arrondir les montants autrement qu'à 2 décimales.
   RAISON : Les centimes d'euro sont comptabilisés — un arrondi mal placé crée une différence de caisse.

5. Si le fichier CSV n'existe pas, le créer avec la ligne d'en-tête.
   RAISON : Le pipeline peut être lancé sur une installation fraîche.

## IMPORTS STANDARDS

```python
import csv
import json
import os
from datetime import datetime
from pathlib import Path
```

## PATTERNS OBLIGATOIRES

```python
# Générer un identifiant d'écriture
from datetime import datetime
ecriture_id = f"ECR-{datetime.now.strftime('%Y%m%d')}-{sequence:04d}"

# Vérifier doublon avant insertion
def already_recorded(csv_path: str, numero_facture: str) -> bool:
    if not Path(csv_path).exists:
        return False
    with open(csv_path, newline="", encoding="utf-8") as f:
        for row in csv.DictReader(f):
            if row.get("numero_facture") == numero_facture:
                return True
    return False

# Ajouter les lignes (append mode)
with open(csv_path, "a", newline="", encoding="utf-8") as f:
    writer = csv.writer(f)
    # Ligne 1 : charge (débit)
    writer.writerow([ecriture_id, date, numero, fournisseur, "60700", "401000",
                     round(montant_ht, 2), f"Facture {fournisseur} - HT"])
    # Ligne 2 : TVA (débit)
    writer.writerow([ecriture_id, date, numero, fournisseur, "44566", "401000",
                     round(tva, 2), f"Facture {fournisseur} - TVA"])
```

## GESTION DES ERREURS DOMAINE

- Input non-JSON              → failed("input_invalide", "Données de validation non parsables")
- statut != "VALIDE"          → failed("facture_invalide", "Facture avec statut {statut} — non comptabilisable")
- Doublon détecté             → failed("doublon_facture", "Facture {numero} déjà enregistrée")
- PermissionError sur CSV     → failed("acces_refuse", "Impossible d'écrire dans {path}")
"""


def manifest() -> dict[str, Any]:
    return {
        "name": "compta-worker",
        "version": "1.0.0",
        "description": (
            "Génère une écriture comptable débit/crédit depuis une facture validée "
            "et l'ajoute au fichier CSV d'export. Vérifie les doublons avant insertion."
        ),
        "execution_mode": "direct",
        "tools_required": ["file_io", "python_executor"],
        "tools_optional": [],
        "packages": [],
        "memory_namespace": "compta-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "record-invoice",
                "name": "Enregistrer une écriture comptable",
                "description": (
                    "Génère l'écriture comptable débit/crédit d'une facture fournisseur validée "
                    "et l'ajoute au fichier CSV export/ecritures-comptables.csv. "
                    "Vérifie les doublons par numéro de facture avant insertion."
                ),
                "input_modes": ["json"],
                "output_modes": ["json"],
            }
        ],
        "tags": ["comptabilite", "ecriture", "csv", "export"],
        "dangerous_tools_allowed": False,
    }


class ComptaWorker:
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 6
    TEMPERATURE = 0.0

    EXPORT_PATH = "export/ecritures-comptables.csv"
    CSV_HEADER = [
        "ecriture_id", "date", "numero_facture", "fournisseur",
        "compte_debit", "compte_credit", "montant", "libelle",
    ]

    async def run(self, task, ctx):
        import json as _json

        parts = task["input"]["parts"]
        input_text = next((p["text"] for p in parts if p.get("type") == "text"), "")

        # Guardrail 1 : input JSON valide
        try:
            validation_result = _json.loads(input_text)
        except (_json.JSONDecodeError, ValueError):
            return AIPResult.failed(
                "input_invalide",
                "Les données de validation reçues ne sont pas du JSON valide.",
            )

        # Guardrail 2 : statut VALIDE requis
        statut = validation_result.get("statut", "INCONNU")
        if statut != "VALIDE":
            return AIPResult.failed(
                "facture_invalide",
                f"Facture avec statut '{statut}' — comptabilisation impossible.",
            )

        # Guardrail 3 : données facture présentes
        facture = validation_result.get("facture", {})
        numero = facture.get("numero")
        if not numero:
            return AIPResult.failed(
                "numero_manquant",
                "Numéro de facture absent des données de validation.",
            )

        try:
            result = await self.react(task, ctx, input_text)
            return result
        except PermissionError as exc:
            return AIPResult.failed(
                "acces_refuse",
                f"Impossible d'écrire dans {self.EXPORT_PATH} : {exc}",
            )
        except Exception as exc:
            return AIPResult.failed("erreur_interne", str(exc))


agent = ComptaWorker()
```

---

## Installation des trois Workers

```bash
apollia-os agent install agents/pdf-invoice-worker.py
apollia-os agent install agents/invoice-validator-worker.py
apollia-os agent install agents/compta-worker.py

# Vérifier
apollia-os agent list --supports-a2a
# pdf-invoice-worker     [Active]   → extract-invoice
# invoice-validator-worker [Active] → validate-invoice
# compta-worker          [Active]   → record-invoice
```
