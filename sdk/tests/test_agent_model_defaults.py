"""Tests for ``@agent(model=...)``, the package level of the request settings."""

import sys
import types
from typing import Any

import pytest
from apollia._internal.manifest import MANIFEST_ATTR
from apollia.agent import agent
from apollia.errors import AgentConfigError
from apollia.skills import skill

_COUNTER = 0


def _agent_class(**decorator_args: Any) -> type:
    global _COUNTER
    _COUNTER += 1
    mod_name = f"test_agent_model_defaults_mod_{_COUNTER}"
    sys.modules[mod_name] = types.ModuleType(mod_name)

    class Notes:
        @skill("notes.take")
        async def take(self, text: str, ctx: Any = None) -> dict[str, str]:
            return {"text": text}

    Notes.__module__ = mod_name
    return agent(name="notes", version="0.1.0", description="desc", **decorator_args)(Notes)


def test_the_package_request_settings_reach_the_manifest() -> None:
    # GIVEN an agent declaring the model defaults its package recommends
    model = {
        "model": "qwen3-8b",
        "preset": "balanced",
        "request": {
            "temperature": 0.2,
            "top_p": 0.9,
            "top_k": 40,
            "repetition_penalty": 1.05,
            "max_tokens": 2048,
            "seed": 7,
        },
    }

    # WHEN the decorator builds the manifest
    manifest = getattr(_agent_class(model=model), MANIFEST_ATTR)

    # THEN the manifest carries them under "model", in the apollia.toml shape
    assert manifest["model"] == model


def test_an_agent_without_model_defaults_has_no_model_key() -> None:
    # GIVEN an agent declaring no model defaults
    # WHEN the decorator builds the manifest
    manifest = getattr(_agent_class(), MANIFEST_ATTR)

    # THEN the manifest says nothing, and the model's own defaults apply
    assert "model" not in manifest


@pytest.mark.parametrize(
    ("model", "message"),
    [
        ({"request": {"temprature": 0.2}}, "unknown key"),
        ({"modle": "qwen3"}, "unknown key"),
        ({"request": {"top_k": 1.5}}, "non-negative int"),
        ({"request": {"temperature": "hot"}}, "must be a number"),
        ({"request": {"temperature": True}}, "must be a number"),
        ({"preset": ""}, "non-empty string"),
        ("balanced", "must be a dict"),
    ],
)
def test_a_malformed_declaration_is_refused_at_import(model: Any, message: str) -> None:
    # GIVEN a malformed model declaration
    # WHEN the decorator is applied
    # THEN it raises AgentConfigError, naming the fault
    with pytest.raises(AgentConfigError, match=message):
        _agent_class(model=model)
