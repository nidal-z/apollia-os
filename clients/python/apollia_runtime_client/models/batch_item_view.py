from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.batch_item_view_input import BatchItemViewInput
    from ..models.batch_item_view_sources_type_0 import BatchItemViewSourcesType0


T = TypeVar("T", bound="BatchItemView")


@_attrs_define
class BatchItemView:
    """One item of a batch.

    Attributes:
        input_ (BatchItemViewInput):
        origin (str): Where it comes from inside the source.
        rank (int): Its position in the source, from 0.
        status (str): `pending`, `running`, `completed`, `failed`, `canceled`, or `awaiting`
            a decision on an access nobody granted.
        updated_at (str): When it last changed state, UTC: for a running item, when its run
            started.
        output (None | str | Unset):
        sources (BatchItemViewSourcesType0 | None | Unset):
        task_id (None | str | Unset): The task that ran it.
        duration_ms (int | None | Unset): How long its last run took, once it ended.
    """

    input_: BatchItemViewInput
    origin: str
    rank: int
    status: str
    updated_at: str
    output: None | str | Unset = UNSET
    sources: BatchItemViewSourcesType0 | None | Unset = UNSET
    task_id: None | str | Unset = UNSET
    duration_ms: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.batch_item_view_sources_type_0 import BatchItemViewSourcesType0

        input_ = self.input_.to_dict()

        origin = self.origin

        rank = self.rank

        status = self.status

        updated_at = self.updated_at

        output: None | str | Unset
        if isinstance(self.output, Unset):
            output = UNSET
        else:
            output = self.output

        sources: dict[str, Any] | None | Unset
        if isinstance(self.sources, Unset):
            sources = UNSET
        elif isinstance(self.sources, BatchItemViewSourcesType0):
            sources = self.sources.to_dict()
        else:
            sources = self.sources

        task_id: None | str | Unset
        if isinstance(self.task_id, Unset):
            task_id = UNSET
        else:
            task_id = self.task_id

        duration_ms: int | None | Unset
        if isinstance(self.duration_ms, Unset):
            duration_ms = UNSET
        else:
            duration_ms = self.duration_ms

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "input": input_,
                "origin": origin,
                "rank": rank,
                "status": status,
                "updatedAt": updated_at,
            }
        )
        if output is not UNSET:
            field_dict["output"] = output
        if sources is not UNSET:
            field_dict["sources"] = sources
        if task_id is not UNSET:
            field_dict["taskId"] = task_id
        if duration_ms is not UNSET:
            field_dict["durationMs"] = duration_ms

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_item_view_input import BatchItemViewInput
        from ..models.batch_item_view_sources_type_0 import BatchItemViewSourcesType0

        d = dict(src_dict)
        input_ = BatchItemViewInput.from_dict(d.pop("input"))

        origin = d.pop("origin")

        rank = d.pop("rank")

        status = d.pop("status")

        updated_at = d.pop("updatedAt")

        def _parse_output(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        output = _parse_output(d.pop("output", UNSET))

        def _parse_sources(data: object) -> BatchItemViewSourcesType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                sources_type_0 = BatchItemViewSourcesType0.from_dict(data)

                return sources_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(BatchItemViewSourcesType0 | None | Unset, data)

        sources = _parse_sources(d.pop("sources", UNSET))

        def _parse_task_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        task_id = _parse_task_id(d.pop("taskId", UNSET))

        def _parse_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        duration_ms = _parse_duration_ms(d.pop("durationMs", UNSET))

        batch_item_view = cls(
            input_=input_,
            origin=origin,
            rank=rank,
            status=status,
            updated_at=updated_at,
            output=output,
            sources=sources,
            task_id=task_id,
            duration_ms=duration_ms,
        )

        batch_item_view.additional_properties = d
        return batch_item_view

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
