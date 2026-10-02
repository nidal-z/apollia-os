"""Validation of the model defaults an agent declares with ``@agent(model=...)``.

The shape is the one ``apollia.toml`` reads under ``[agents.defaults.model]``:
``model`` and ``preset`` names, and ``request`` settings. The runtime refuses
an unknown key, so the decorator refuses it first, at import time.
"""

from typing import Any

from apollia.errors import AgentConfigError

__all__ = ["check_model_defaults"]

_MODEL_KEYS: frozenset[str] = frozenset({"model", "preset", "request"})

# Request setting -> accepted types. ``bool`` is excluded separately because it
# is an ``int`` subclass.
_FLOAT_KEYS: frozenset[str] = frozenset({"temperature", "top_p", "repetition_penalty"})
_INT_KEYS: frozenset[str] = frozenset({"top_k", "max_tokens", "seed"})
_REQUEST_KEYS: frozenset[str] = _FLOAT_KEYS | _INT_KEYS


def _check_request(value: object) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise AgentConfigError(
            f"@agent: 'model.request' must be a dict, got {type(value).__name__}"
        )
    unknown = sorted(set(value) - _REQUEST_KEYS)
    if unknown:
        accepted = ", ".join(sorted(_REQUEST_KEYS))
        raise AgentConfigError(
            f"@agent: unknown key(s) in 'model.request': {unknown}; accepted: [{accepted}]"
        )
    out: dict[str, Any] = {}
    for key, setting in value.items():
        if setting is None:
            continue
        if isinstance(setting, bool):
            raise AgentConfigError(f"@agent: 'model.request.{key}' must be a number")
        if key in _INT_KEYS:
            if not isinstance(setting, int) or setting < 0:
                raise AgentConfigError(
                    f"@agent: 'model.request.{key}' must be a non-negative int, got {setting!r}"
                )
        elif not isinstance(setting, (int, float)):
            raise AgentConfigError(
                f"@agent: 'model.request.{key}' must be a number, got {setting!r}"
            )
        out[key] = setting
    return out


def check_model_defaults(value: object) -> dict[str, Any] | None:
    """Validate ``@agent(model=...)`` and return its canonical form.

    Returns ``None`` when nothing is declared. Raises ``AgentConfigError`` on an
    unknown key, a non-string name, or a request setting of the wrong type.
    """
    if value is None:
        return None
    if not isinstance(value, dict):
        raise AgentConfigError(f"@agent: 'model' must be a dict, got {type(value).__name__}")
    unknown = sorted(set(value) - _MODEL_KEYS)
    if unknown:
        accepted = ", ".join(sorted(_MODEL_KEYS))
        raise AgentConfigError(
            f"@agent: unknown key(s) in 'model': {unknown}; accepted: [{accepted}]"
        )
    out: dict[str, Any] = {}
    for key in ("model", "preset"):
        name = value.get(key)
        if name is None:
            continue
        if not isinstance(name, str) or not name:
            raise AgentConfigError(f"@agent: 'model.{key}' must be a non-empty string")
        out[key] = name
    request = value.get("request")
    if request is not None:
        out["request"] = _check_request(request)
    return out
