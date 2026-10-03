from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="InjectedMemoryEntryView")


@_attrs_define
class InjectedMemoryEntryView:
    """One memory entry an agent recalled during a turn.

    Attributes:
        content_preview (str): Short preview of the entry content.
        id (str): Source memory entry id.
        injection_reason (str): Why the agent recalled it.
        namespace (str): Namespace of origin.
        relevance_score (float): Relevance score in `[0.0, 1.0]` when available.
    """

    content_preview: str
    id: str
    injection_reason: str
    namespace: str
    relevance_score: float
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        content_preview = self.content_preview

        id = self.id

        injection_reason = self.injection_reason

        namespace = self.namespace

        relevance_score = self.relevance_score

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "content_preview": content_preview,
                "id": id,
                "injection_reason": injection_reason,
                "namespace": namespace,
                "relevance_score": relevance_score,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        content_preview = d.pop("content_preview")

        id = d.pop("id")

        injection_reason = d.pop("injection_reason")

        namespace = d.pop("namespace")

        relevance_score = d.pop("relevance_score")

        injected_memory_entry_view = cls(
            content_preview=content_preview,
            id=id,
            injection_reason=injection_reason,
            namespace=namespace,
            relevance_score=relevance_score,
        )

        injected_memory_entry_view.additional_properties = d
        return injected_memory_entry_view

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
