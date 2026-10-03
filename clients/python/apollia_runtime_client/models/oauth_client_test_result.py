from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OauthClientTestResult")


@_attrs_define
class OauthClientTestResult:
    """The verdict of a non-interactive check of a provider's OAuth client.

    A full validation (that the authorization server accepts the client) needs
    an interactive consent and a token exchange, which cannot run headless.
    This is the strongest safe check: credentials present, well-formed, and the
    provider's authorization server reachable.

        Attributes:
            detail (str): Human-readable summary: what passed, or the first failing check.
            ok (bool): True when every check passed.
    """

    detail: str
    ok: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        detail = self.detail

        ok = self.ok

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "detail": detail,
                "ok": ok,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        detail = d.pop("detail")

        ok = d.pop("ok")

        oauth_client_test_result = cls(
            detail=detail,
            ok=ok,
        )

        oauth_client_test_result.additional_properties = d
        return oauth_client_test_result

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
