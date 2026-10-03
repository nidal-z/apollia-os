from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.batch_access import BatchAccess


T = TypeVar("T", bound="BatchAccessesView")


@_attrs_define
class BatchAccessesView:
    """The sensitive accesses of an agent.

    Attributes:
        accesses (list[BatchAccess]):
        agent (str):
    """

    accesses: list[BatchAccess]
    agent: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        accesses = []
        for accesses_item_data in self.accesses:
            accesses_item = accesses_item_data.to_dict()
            accesses.append(accesses_item)

        agent = self.agent

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accesses": accesses,
                "agent": agent,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_access import BatchAccess

        d = dict(src_dict)
        accesses = []
        _accesses = d.pop("accesses")
        for accesses_item_data in _accesses:
            accesses_item = BatchAccess.from_dict(accesses_item_data)

            accesses.append(accesses_item)

        agent = d.pop("agent")

        batch_accesses_view = cls(
            accesses=accesses,
            agent=agent,
        )

        batch_accesses_view.additional_properties = d
        return batch_accesses_view

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
