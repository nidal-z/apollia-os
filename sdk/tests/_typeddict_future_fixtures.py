"""TypedDict fixture that DELIBERATELY uses PEP 563 stringified annotations.

This is the anti-pattern that ``_typeddict_schema`` must detect: a payload
TypedDict defined in a module with ``from __future__ import annotations``.
Under PEP 563 every annotation becomes a string, so ``__required_keys__``
wrongly counts ``NotRequired`` fields as required. It exists only to prove the
schema builder warns and still recovers the correct split via ``get_type_hints``.
Do NOT copy this into real worker ``schemas.py`` files.
"""

from __future__ import annotations

from typing import NotRequired, TypedDict


class FutureAnnotatedTD(TypedDict):
    name: str
    color: NotRequired[str]
