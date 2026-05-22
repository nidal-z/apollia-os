# Capstone : implémentation des workers

Trois workers. Chacun ~80 lignes. On commence par `web-research`, on enchaîne avec `crm-lookup`, on finit avec `meeting-prep`. Tous suivent le même pattern : `@agent` + 2 ou 3 `@skill`, TypedDict canon, `DomainError` typées.

---

## Worker 1 : `web-research`

### `web-research/schemas.py`

Pas de `from __future__ import annotations` (PEP 563 casserait `__required_keys__`).

```python
from typing import TypedDict


class CompanyInfo(TypedDict):
    name: str
    industry: str
    size_estimate: str
    headquarters: str
    description: str


class SignalEntry(TypedDict):
    date: str         # ISO 8601
    title: str
    source: str
    url: str
    summary: str
```

### `web-research/web_research.py`

```python
from typing import Annotated

from apollia import DomainError, agent, skill
from apollia.types import Ctx
from schemas import CompanyInfo, SignalEntry


@agent(
    name="web-research",
    version="0.1.0",
    description="Public web research about a company.",
    agent_type="worker",
    tools_required=("web_search", "web_read"),
    datasources=("trusted_news_sources",),
)
class WebResearch:
    @skill(
        "web.research.company",
        description="Find general public info about a company (industry, size, HQ).",
        examples=[{"company_name": "Acme Corp"}],
    )
    async def research_company(
        self,
        company_name: Annotated[str, "Legal or commercial name of the company."],
        ctx: Ctx,
    ) -> CompanyInfo:
        results = await ctx.tools.call(
            "web_search",
            input={"query": f'"{company_name}" entreprise siège effectif'},
        )
        if not results.get("hits"):
            raise DomainError("COMPANY_NOT_FOUND", f"No public info on {company_name}")

        top = results["hits"][0]
        page = await ctx.tools.call("web_read", input={"url": top["url"]})

        # Real impl : LLM summarization. Here we stub for the book.
        return {
            "name": company_name,
            "industry": "Unknown (LLM summarization step)",
            "size_estimate": "Unknown",
            "headquarters": "Unknown",
            "description": page.get("content", "")[:500],
        }

    @skill(
        "web.research.signals",
        description="Find recent news signals about a company (last 90 days).",
        examples=[{"company_name": "Acme Corp", "max_signals": 5}],
    )
    async def research_signals(
        self,
        company_name: Annotated[str, "Legal or commercial name of the company."],
        max_signals: Annotated[int, "Maximum number of signals to return (default 5)."] = 5,
        ctx: Ctx,
    ) -> dict:
        trusted = await ctx.datasources.get("trusted_news_sources")

        signals: list[SignalEntry] = []
        results = await ctx.tools.call(
            "web_search",
            input={"query": f'"{company_name}" actualité 2026', "max_results": max_signals * 2},
        )

        for hit in results.get("hits", [])[:max_signals]:
            if not any(src in hit["url"] for src in trusted["domains"]):
                continue
            page = await ctx.tools.call("web_read", input={"url": hit["url"]})
            signals.append({
                "date": hit.get("date", "Unknown"),
                "title": hit["title"],
                "source": hit["url"],
                "url": hit["url"],
                "summary": page.get("content", "")[:280],
            })

        return {"company_name": company_name, "signals": signals}

    @skill(
        "web.research.linkedin",
        description="Find a company's LinkedIn page and key people.",
        examples=[{"company_name": "Acme Corp"}],
    )
    async def research_linkedin(
        self,
        company_name: Annotated[str, "Company name."],
        ctx: Ctx,
    ) -> dict:
        results = await ctx.tools.call(
            "web_search",
            input={"query": f'site:linkedin.com/company "{company_name}"'},
        )
        if not results.get("hits"):
            return {"company_name": company_name, "linkedin_url": None, "key_people": []}

        # Production : crawl LinkedIn API or pre-fetched dataset.
        return {
            "company_name": company_name,
            "linkedin_url": results["hits"][0]["url"],
            "key_people": [],
        }
```

### `web-research/datasources/trusted_news_sources.yaml`

```yaml
domains:
  - lesechos.fr
  - latribune.fr
  - usine-nouvelle.com
  - linkedin.com
  - bfmtv.com
```

---

## Worker 2 : `crm-lookup`

### `crm-lookup/schemas.py`

```python
from typing import TypedDict


class ContactRecord(TypedDict):
    full_name: str
    job_title: str
    email: str
    last_contact_date: str


class HistoryEntry(TypedDict):
    date: str
    type: str   # "email", "call", "meeting", "quote"
    summary: str
```

### `crm-lookup/crm_lookup.py`

```python
from typing import Annotated

from apollia import DomainError, agent, skill
from apollia.types import Ctx
from schemas import ContactRecord, HistoryEntry


HUBSPOT_API = "https://api.hubapi.com/crm/v3/objects"


@agent(
    name="crm-lookup",
    version="0.1.0",
    description="Read-only CRM lookup (HubSpot).",
    agent_type="worker",
    secrets=("hubspot_api_token",),
    tools_required=("web_read",),
)
class CrmLookup:
    @skill(
        "crm.lookup.account",
        description="Look up contacts for a company in HubSpot CRM.",
        examples=[{"company_name": "Acme Corp"}],
    )
    async def lookup_account(
        self,
        company_name: Annotated[str, "Company name as known in CRM."],
        ctx: Ctx,
    ) -> dict:
        token = ctx.secrets.get("hubspot_api_token")
        if not token:
            raise DomainError("CONFIG", "hubspot_api_token not configured")

        # Real call would use HubSpot search API. Stubbed for the book.
        url = f"{HUBSPOT_API}/companies/search?q={company_name}"
        response = await ctx.tools.call(
            "web_read",
            input={"url": url, "headers": {"Authorization": f"Bearer {token}"}},
        )
        if response.get("status_code", 0) >= 400:
            raise DomainError("CRM_ERROR", f"HubSpot lookup failed: {response.get('status_code')}")

        # Parse and return ; format depends on real HubSpot response shape.
        contacts: list[ContactRecord] = []
        return {"company_name": company_name, "contacts": contacts}

    @skill(
        "crm.lookup.history",
        description="Fetch interaction history with a company contact.",
        examples=[{"contact_email": "pierre.martin@acmecorp.fr", "since_days": 365}],
    )
    async def lookup_history(
        self,
        contact_email: Annotated[str, "Email of the contact in CRM."],
        since_days: Annotated[int, "Look back window in days (default 365)."] = 365,
        ctx: Ctx,
    ) -> dict:
        token = ctx.secrets.get("hubspot_api_token")
        if not token:
            raise DomainError("CONFIG", "hubspot_api_token not configured")

        # Real impl : query HubSpot engagements API.
        history: list[HistoryEntry] = []
        return {"contact_email": contact_email, "history": history}
```

---

## Worker 3 : `meeting-prep`

### `meeting-prep/schemas.py`

```python
from typing import TypedDict


class BriefPayload(TypedDict):
    company_name: str
    meeting_when: str
    company_info: dict
    signals: list
    crm_contacts: list
    crm_history: list
```

### `meeting-prep/meeting_prep.py`

```python
from typing import Annotated

from apollia import DomainError, agent, skill
from apollia.types import Ctx
from schemas import BriefPayload


@agent(
    name="meeting-prep",
    version="0.1.0",
    description="Format the final meeting briefing.",
    agent_type="worker",
    templates=("brief", "questions"),
)
class MeetingPrep:
    @skill(
        "prep.build_brief",
        description="Render the full meeting briefing as Markdown.",
        examples=[{
            "company_name": "Acme Corp",
            "meeting_when": "demain 10:00",
            "company_info": {"name": "Acme Corp"},
            "signals": [],
            "crm_contacts": [],
            "crm_history": [],
        }],
    )
    async def build_brief(self, payload: BriefPayload, ctx: Ctx) -> dict:
        if not payload.get("company_name"):
            raise DomainError("EMPTY_COMPANY", "company_name must not be empty")

        markdown = ctx.templates.render(
            "brief",
            company_name=payload["company_name"],
            meeting_when=payload["meeting_when"],
            company_info=payload["company_info"],
            signals=payload["signals"],
            crm_contacts=payload["crm_contacts"],
            crm_history=payload["crm_history"],
        )
        return {"markdown": markdown, "company_name": payload["company_name"]}

    @skill(
        "prep.format_questions",
        description="Format a list of open questions to ask at the meeting.",
        examples=[{"company_name": "Acme Corp", "themes": ["funding", "hiring"]}],
    )
    async def format_questions(
        self,
        company_name: Annotated[str, "Company name."],
        themes: Annotated[list[str], "Topics to probe. Each topic generates 1-2 questions."],
        ctx: Ctx,
    ) -> dict:
        markdown = ctx.templates.render(
            "questions",
            company_name=company_name,
            themes=themes,
        )
        return {"markdown": markdown}
```

### `meeting-prep/templates/brief.j2`

```jinja
# RDV {{ company_name }}, {{ meeting_when }}

## L'entreprise
{{ company_info.description }}

## Signaux récents
{% for signal in signals %}
- {{ signal.date }} : {{ signal.title }} ({{ signal.source }}).
{% else %}
- Aucun signal récent.
{% endfor %}

## Contacts CRM
{% for contact in crm_contacts %}
- {{ contact.full_name }} ({{ contact.job_title }}, {{ contact.email }}).
{% endfor %}

## Historique récent
{% for entry in crm_history %}
- {{ entry.date }} : {{ entry.type }} - {{ entry.summary }}.
{% endfor %}
```

### `meeting-prep/templates/questions.j2`

```jinja
## Questions à poser à {{ company_name }}

{% for theme in themes %}
- À propos de **{{ theme }}** : ?
{% endfor %}
```

---

## Installation

```bash
apollia-os agent install ./agents/web-research/web_research.py
apollia-os agent install ./agents/crm-lookup/crm_lookup.py
apollia-os agent install ./agents/meeting-prep/meeting_prep.py

# Configurer le secret CRM (prompt interactif)
apollia-os tools credentials set
# Saisir la clé : hubspot_api_token
# Saisir la valeur : pat-eu1-...
```

Vérification :

```bash
apollia-os agent list
#   NAME              VERSION    STATUS    AUTO-LOAD  SOURCE
#   web-research      0.1.0      active    yes        installed
#   crm-lookup        0.1.0      active    yes        installed
#   meeting-prep      0.1.0      active    yes        installed

apollia-os a2a skills | grep -E "^(web|crm|prep)\."
```

Si un agent échoue à l'installation, `python -m apollia inspect <fichier>.py` donnera le détail.

---

## Tests fonctionnels

Un fichier de test par worker, le pattern est identique. Exemple pour `web-research` :

```python
# tests/test_web_research.py
import pytest
from apollia.testing import mock, assert_result_completed, assert_result_failed

from web_research import WebResearch


@pytest.mark.asyncio
async def test_research_company_returns_basic_info():
    agent, ctx = mock(WebResearch)
    ctx.tools.responses = {
        "web_search": {"hits": [{"url": "https://example.com/acme", "title": "Acme"}]},
        "web_read": {"content": "Acme Corp is a precision parts manufacturer."},
    }
    ctx.datasources.values = {"trusted_news_sources": {"domains": ["example.com"]}}

    result = await agent.invoke_skill("web.research.company", company_name="Acme")

    assert_result_completed(result)
    data = result["output"][0]["data"]
    assert "Acme" in data["description"]


@pytest.mark.asyncio
async def test_research_company_raises_when_nothing_found():
    agent, ctx = mock(WebResearch)
    ctx.tools.responses = {"web_search": {"hits": []}}

    result = await agent.invoke_skill("web.research.company", company_name="ZZZ")

    assert_result_failed(result, code="COMPANY_NOT_FOUND")
```

Les trois workers sont testables avec `pytest tests/` sans démarrer le runtime ni le LLM réel.

---

## Prochaine étape

Passez au [chapitre 40](40-capstone-director-result.md) pour le director et le résultat final.
