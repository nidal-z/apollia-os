"""TypedDict fixtures for inference tests.

This module intentionally omits ``from __future__ import annotations`` so
that ``TypedDict.__required_keys__`` / ``__optional_keys__`` can be
correctly populated by interpreting ``NotRequired[T]`` / ``Required[T]``
at class-creation time (PEP 655). Worker ``schemas.py`` files follow the
same convention.
"""

from typing import NotRequired, Required, TypedDict


class PlainTD(TypedDict):
    a: int
    b: str


class NotRequiredTD(TypedDict):
    name: str
    data: list[float]
    color: NotRequired[str]


class RequiredTotalFalseTD(TypedDict, total=False):
    name: Required[str]
    size: int


class TotalFalseNoRequiredTD(TypedDict, total=False):
    name: str
    size: int
