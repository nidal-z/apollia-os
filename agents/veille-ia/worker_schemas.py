"""Canonical TypedDicts pour les skills A2A LLM-facing de veille-ia.

Ces TypedDicts pilotent l'inférence JSON Schema du SDK Apollia
(``sdk/apollia/_internal/inference.py``). Le LLM mid-market voit ainsi la
forme exacte des structures circulant entre director ↔ workers
(``articles``, ``known_entities``, ``scoring``, etc.) au lieu d'un
``object`` opaque.

⚠️ **Convention PEP 655 + PEP 563** : ce module n'utilise *pas*
``from __future__ import annotations``. ``TypedDict.__required_keys__``
est calculé à la création de la classe en inspectant les annotations
brutes ; sous PEP 563 (stringified annotations), ``NotRequired[T]``
n'est plus détecté et tous les champs deviennent "requis", ce qui
casse le ``required: [...]`` du schéma produit par le SDK.

Les validations runtime (auto-repair Pydantic, coercions LLM locales)
restent dans ``schemas.py`` (Pydantic). Ce module ne sert qu'à
documenter le **contrat skill-level** vu par le LLM.
"""

from typing import Any, Literal, NotRequired, TypedDict


# ─── Axes ──────────────────────────────────────────────────────────────────


Axis = Literal["tech", "competitive"]
EntityType = Literal["company", "product", "event", "topic"]
EntityCategory = Literal["direct", "indirect", "neutral"]
ThreatLevel = Literal["low", "medium", "high"]


# ─── Article (input → workers, sortie web-search) ──────────────────────────


class Article(TypedDict):
    """Article de veille — payload circulant entre workers.

    Sortie minimale de ``research.search_and_extract`` ; entrée de
    ``research.extract_entities`` et ``research.synthesize_report``.
    Les champs ``score``/``score_stars``/``entities``/``impact_*``/
    ``is_critical``/``matched_criteria`` ne sont remplis qu'à partir de
    l'étape synthesis ; ils restent ``NotRequired`` au niveau du contrat.
    """

    title: str
    url: str
    source: str
    excerpt: str
    axis: Axis
    hash: NotRequired[str]
    score: NotRequired[int]
    score_stars: NotRequired[str]
    entities: NotRequired[list[str]]
    impact_apollia: NotRequired[str]
    impact_for_user: NotRequired[str]
    is_critical: NotRequired[bool]
    matched_criteria: NotRequired[list[str]]
    published_at: NotRequired[str]


# ─── Entity (input known_entities, output extract_entities) ────────────────


class Entity(TypedDict):
    """Entité monitorée — companies, products, events, topics.

    Identifiant canonique : ``entity:{type}:{id}`` côté mémoire ; le champ
    ``id`` est ici le segment ``{id}`` kebab-case (cf. schemas.py Pydantic).
    """

    id: str
    type: EntityType
    name: str
    category: NotRequired[EntityCategory]
    threat_level: NotRequired[ThreatLevel]
    summary: NotRequired[str]
    score: NotRequired[int]
    source_url: NotRequired[str]
    first_seen: NotRequired[str]
    last_event_date: NotRequired[str]


# ─── Scoring (config YAML chargée par le director) ─────────────────────────


class ScoringCriterion(TypedDict):
    """Un critère pondéré de scoring (chargé depuis ``scoring.yaml``)."""

    id: str
    weight: int
    keywords: NotRequired[list[str]]
    description: NotRequired[str]


class ScoringConfig(TypedDict):
    """Config de scoring — résultat du chargement de ``scoring.yaml``.

    ``critical_threshold`` : score >= seuil ⇒ article remonté en
    ``critical_findings``. ``include_threshold`` : articles strictement
    en dessous ne sont pas inclus dans le rapport final.
    """

    criteria: list[ScoringCriterion]
    critical_threshold: NotRequired[int]
    include_threshold: NotRequired[int]


# ─── UserContext (snapshot ctx.profile pour personnalisation rapport) ──────


class UserContext(TypedDict, total=False):
    """Snapshot du profil utilisateur (ADR-087, clés plates ``user.*``).

    Toutes les clés sont optionnelles : l'absence d'un champ signifie
    juste que le profil ne l'a pas renseigné. Le worker synthesis adapte
    l'``executive_summary`` selon ``user.role`` (CTO → tech, founder →
    business, …).
    """

    # Préfixe "user." conservé pour rester aligné avec ce que le director
    # injecte (cf. veille-ia-agent.py:_step_load_user_context).
    user_role: str  # alias clé `user.role`
    user_tech_stack: str
    user_domain_sector: str
    user_agents_hitl: str


# ─── ArticleEntityMap (mapping URL → entity IDs) ───────────────────────────


# Pas de TypedDict possible (clés dynamiques = URLs). On expose un alias
# explicite pour clarifier l'intention dans les signatures.
ArticleEntityMap = dict[str, list[str]]


# ─── Outputs des skills (pour cohérence doc — utilisés par tests/eval) ─────


class ExtractEntitiesOutput(TypedDict):
    """Output canonique de ``research.extract_entities``."""

    entities: list[Entity]
    article_to_entities: ArticleEntityMap


class SynthesizeReportOutput(TypedDict):
    """Output canonique de ``research.synthesize_report``.

    Note : le LLM produit un objet conforme à ``VeilleReport``
    (cf. ``schemas.py`` Pydantic) — ce TypedDict décrit l'enveloppe vue
    par les callers A2A. La validation stricte reste Pydantic côté
    director (``_validate_with_repair``).
    """

    date: str
    executive_summary: str
    articles_tech: list[Article]
    articles_competitive: list[Article]
    critical_findings: list[Article]
    new_entities: list[str]
    metrics: dict[str, Any]
