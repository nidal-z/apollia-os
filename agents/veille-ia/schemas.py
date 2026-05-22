"""Schemas Pydantic pour veille-ia v0.1.0 (Pilier 1).

LLM produit JSON conforme à ces schemas, validation post-gen avec auto-repair (max 3 retries).
"""

from __future__ import annotations
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator


class EntitySignal(BaseModel):
    """Un signal observé pour une entité à un instant t."""

    date: str = Field(..., description="Date ISO (YYYY-MM-DD)")
    summary: str = Field(..., max_length=500)
    source_url: str = ""
    score: int = Field(..., ge=1, le=5)


class Entity(BaseModel):
    """Entité monitorée (concurrent, produit, événement, sujet)."""

    id: str = Field(..., description="kebab-case unique")
    type: Literal["company", "product", "event", "topic"]
    name: str
    category: str = "direct"
    threat_level: Literal["low", "medium", "high"] = "medium"
    first_seen: str = Field(..., description="Date ISO (YYYY-MM-DD)")
    last_event_date: str = Field(..., description="Date ISO (YYYY-MM-DD)")
    signals: list[EntitySignal] = []


class Article(BaseModel):
    """Article extrait par le worker web-search.

    Tolère les LLM locaux qui :
    - confondent `impact_*` (string narratif) avec un score numérique → coercition int→str.
    - omettent ou tronquent certains champs → defaults.

    `coerce_numbers_to_str=True` : feature Pydantic v2.5+ qui force int/float → str
    automatiquement à la validation (filet de sécurité indépendant des field_validator).
    """

    model_config = ConfigDict(coerce_numbers_to_str=True)

    title: str = Field(..., min_length=1, max_length=300)
    url: str
    source: str = Field(..., description="Hostname ou nom de la source")
    excerpt: str = Field(..., max_length=2000)
    score: int = Field(..., ge=1, le=5)
    score_stars: str = Field(default="", description="Représentation ★ étoiles")
    axis: Literal["tech", "competitive"]
    entities: list[str] = Field(default_factory=list, description="IDs d'entités liées (entity:company:n8n, etc.)")
    impact_apollia: str = Field(default="", max_length=500)
    impact_for_user: str = Field(default="", max_length=500)
    is_critical: bool = False
    matched_criteria: list[str] = Field(default_factory=list, description="IDs des critères de scoring matchés")

    @field_validator("impact_apollia", "impact_for_user", "score_stars", mode="before")
    @classmethod
    def _coerce_to_string(cls, v: Any) -> str:
        """LLMs locaux retournent parfois un int (score) au lieu d'un str (description)."""
        if v is None:
            return ""
        if isinstance(v, (int, float)):
            return str(v)
        return v

    @field_validator("entities", "matched_criteria", mode="before")
    @classmethod
    def _coerce_to_list(cls, v: Any) -> list:
        """Tolère None ou string unique (LLM convertit liste à 1 élément en string)."""
        if v is None:
            return []
        if isinstance(v, str):
            return [v] if v else []
        return v

    @field_validator("score", mode="before")
    @classmethod
    def _coerce_score(cls, v: Any) -> int:
        """LLM peut retourner '5' (str) ou 5.0 (float) — clamper à 1-5."""
        if isinstance(v, str):
            try:
                v = int(float(v))
            except (ValueError, TypeError):
                return 3
        if isinstance(v, float):
            v = int(v)
        if not isinstance(v, int) or v < 1:
            return 1
        if v > 5:
            return 5
        return v


class VeilleReport(BaseModel):
    """Rapport quotidien complet, produit par le synthesis-worker et rendu par le director via Jinja2.

    Tolère les LLM locaux qui :
    - retournent `critical_findings` comme liste de strings narratives au lieu d'Articles.
    - mettent des ints (scores) dans des champs string (`impact_*`).
    - tronquent ou omettent certains champs.

    Trois niveaux de défense :
    1. `model_config = ConfigDict(coerce_numbers_to_str=True)` — coercion native Pydantic.
    2. `_preprocess_dict` (model_validator before) — coerce/filtre nested avant validation des sub-objets.
    3. `field_validator(before)` par champ — defense en profondeur.
    """

    model_config = ConfigDict(coerce_numbers_to_str=True)

    date: str = Field(..., description="Date ISO du run (YYYY-MM-DD)")
    executive_summary: str = Field(..., min_length=10, max_length=500)
    articles_tech: list[Article] = Field(default_factory=list)
    articles_competitive: list[Article] = Field(default_factory=list)
    critical_findings: list[Article] = Field(default_factory=list)
    new_entities: list[str] = Field(default_factory=list, description="IDs d'entités vues pour la 1re fois")
    metrics: dict = Field(default_factory=dict)

    @model_validator(mode="before")
    @classmethod
    def _preprocess_dict(cls, data: Any) -> Any:
        """Pre-process le dict brut AVANT toute validation des sub-objets.

        Application coerce et filtres globaux sur la structure complète :
        - `critical_findings` : filtre les strings narratives, garde uniquement dicts/Articles.
        - `articles_tech` / `articles_competitive` : coerce ints en strings sur impact_*, score_stars.
          (Filet supplémentaire au cas où les field_validator d'Article ne s'appliqueraient pas.)
        """
        if not isinstance(data, dict):
            return data

        # Filtre critical_findings : enlève les strings narratives
        if "critical_findings" in data and isinstance(data["critical_findings"], list):
            data["critical_findings"] = [
                item for item in data["critical_findings"] if isinstance(item, dict)
            ]

        # Coerce ints/floats → strings dans les sub-articles
        for list_key in ("articles_tech", "articles_competitive", "critical_findings"):
            if list_key not in data or not isinstance(data[list_key], list):
                continue
            for article in data[list_key]:
                if not isinstance(article, dict):
                    continue
                for str_key in ("impact_apollia", "impact_for_user", "score_stars",
                                "title", "url", "source", "excerpt"):
                    val = article.get(str_key)
                    if isinstance(val, (int, float)):
                        article[str_key] = str(val)
                    elif val is None:
                        article[str_key] = ""
                # axis fallback si manquant ou invalide
                if article.get("axis") not in ("tech", "competitive"):
                    article["axis"] = "tech" if list_key == "articles_tech" else "competitive"

        return data

    @field_validator("critical_findings", mode="before")
    @classmethod
    def _filter_critical_findings(cls, v: Any) -> list:
        """Filet supplémentaire en cas de bypass du model_validator."""
        if v is None:
            return []
        if not isinstance(v, list):
            return []
        return [item for item in v if isinstance(item, (dict, Article))]

    @field_validator("metrics", mode="before")
    @classmethod
    def _coerce_metrics(cls, v: Any) -> dict:
        """LLM peut omettre metrics ou retourner None."""
        return v if isinstance(v, dict) else {}

    @field_validator("new_entities", mode="before")
    @classmethod
    def _coerce_new_entities(cls, v: Any) -> list:
        if not isinstance(v, list):
            return []
        return [str(x) for x in v if x]

    @field_validator("executive_summary", mode="before")
    @classmethod
    def _coerce_summary(cls, v: Any) -> str:
        """Garantir une string non vide (min_length=10 sinon)."""
        if v is None:
            return "Synthèse en cours."
        if not isinstance(v, str):
            v = str(v)
        if len(v.strip()) < 10:
            return v.strip() + " " * (10 - len(v.strip())) if v.strip() else "Synthèse non générée."
        return v


class WorkerSearchResult(BaseModel):
    """Output du web-search-worker (skill `search-and-extract`)."""

    articles: list[Article]
    total_found: int
    skipped_dupes: int


class WorkerEntityExtractionResult(BaseModel):
    """Output du entity-extraction-worker (skill `extract-entities`)."""

    entities: list[Entity]
    article_to_entities: dict[str, list[str]] = Field(
        default_factory=dict, description="Mapping URL -> entity IDs"
    )


class WorkerSynthesisResult(BaseModel):
    """Output du synthesis-worker (skill `synthesize-report`)."""

    report: VeilleReport
