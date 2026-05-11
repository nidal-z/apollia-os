"""State machine pour veille-ia v3.0.0 (Pilier 2).

Le director dispatche sur ces steps. Chaque step = 1 méthode privée Python avec contrat clair :
- Pré-conditions
- Action (LLM ou déterministe)
- Post-conditions
- Politique d'erreur
"""

from enum import Enum


class VeilleStep(Enum):
    INIT = "init"
    LOAD_DATASOURCES = "load_datasources"      # déterministe (YAML parse)
    LOAD_USER_CONTEXT = "load_user_context"    # déterministe (recall user.*)
    BOOTSTRAP_CHECK = "bootstrap_check"        # déterministe (TTL check + remember)
    LOAD_ENTITIES = "load_entities"            # déterministe (memory.search entity:)
    SEARCH_TECH = "search_tech"                # délègue au web-search-worker (A2A)
    SEARCH_COMPETITIVE = "search_competitive"  # délègue au web-search-worker (A2A)
    EXTRACT_ENTITIES = "extract_entities"      # délègue au entity-extraction-worker (A2A)
    SCORE_AND_RANK = "score_and_rank"          # délègue au synthesis-worker (A2A — applique scoring.yaml + LLM)
    DETECT_CRITICAL = "detect_critical"        # déterministe (filter is_critical)
    GENERATE_REPORT = "generate_report"        # déterministe (Jinja2 render)
    PERSIST_MEMORY = "persist_memory"          # déterministe (record épisodique + entity upsert)
    WRITE_FILE = "write_file"                  # déterministe (file_write)
    NOTIFY = "notify"                          # déterministe (ctx.notify.publish)
    DONE = "done"
