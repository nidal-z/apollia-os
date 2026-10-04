from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.fail_closed_type_0 import FailClosedType0
    from ..models.fail_closed_type_1 import FailClosedType1


T = TypeVar("T", bound="GovernanceStateBody")


@_attrs_define
class GovernanceStateBody:
    """Answer of `GET /api/v1/governance/state`.

    Attributes:
        fail_closed (FailClosedType0 | FailClosedType1 | None | Unset):
    """

    fail_closed: FailClosedType0 | FailClosedType1 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.fail_closed_type_0 import FailClosedType0
        from ..models.fail_closed_type_1 import FailClosedType1

        fail_closed: dict[str, Any] | None | Unset
        if isinstance(self.fail_closed, Unset):
            fail_closed = UNSET
        elif isinstance(self.fail_closed, FailClosedType0):
            fail_closed = self.fail_closed.to_dict()
        elif isinstance(self.fail_closed, FailClosedType1):
            fail_closed = self.fail_closed.to_dict()
        else:
            fail_closed = self.fail_closed

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if fail_closed is not UNSET:
            field_dict["fail_closed"] = fail_closed

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.fail_closed_type_0 import FailClosedType0
        from ..models.fail_closed_type_1 import FailClosedType1

        d = dict(src_dict)

        def _parse_fail_closed(data: object) -> FailClosedType0 | FailClosedType1 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_fail_closed_type_0 = FailClosedType0.from_dict(data)

                return componentsschemas_fail_closed_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_fail_closed_type_1 = FailClosedType1.from_dict(data)

                return componentsschemas_fail_closed_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(FailClosedType0 | FailClosedType1 | None | Unset, data)

        fail_closed = _parse_fail_closed(d.pop("fail_closed", UNSET))

        governance_state_body = cls(
            fail_closed=fail_closed,
        )

        governance_state_body.additional_properties = d
        return governance_state_body

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
