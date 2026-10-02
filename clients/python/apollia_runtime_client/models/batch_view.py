from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.batch_view_source import BatchViewSource


T = TypeVar("T", bound="BatchView")


@_attrs_define
class BatchView:
    """A batch: what it was launched with, its state and its counters.

    Attributes:
        agent (str):
        batch_id (str):
        canceled (int):
        completed (int):
        concurrency (int):
        created_at (str):
        failed (int):
        grants (list[str]):
        max_failures (int): Failed items tolerated before the batch is interrupted.
        source (BatchViewSource): The source descriptor, `{kind, location, options}`.
        state (str): `running`, `completed` or `interrupted`.
        total (int):
        updated_at (str):
        ended_at (None | str | Unset):
    """

    agent: str
    batch_id: str
    canceled: int
    completed: int
    concurrency: int
    created_at: str
    failed: int
    grants: list[str]
    max_failures: int
    source: BatchViewSource
    state: str
    total: int
    updated_at: str
    ended_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent = self.agent

        batch_id = self.batch_id

        canceled = self.canceled

        completed = self.completed

        concurrency = self.concurrency

        created_at = self.created_at

        failed = self.failed

        grants = self.grants

        max_failures = self.max_failures

        source = self.source.to_dict()

        state = self.state

        total = self.total

        updated_at = self.updated_at

        ended_at: None | str | Unset
        if isinstance(self.ended_at, Unset):
            ended_at = UNSET
        else:
            ended_at = self.ended_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent": agent,
                "batchId": batch_id,
                "canceled": canceled,
                "completed": completed,
                "concurrency": concurrency,
                "createdAt": created_at,
                "failed": failed,
                "grants": grants,
                "maxFailures": max_failures,
                "source": source,
                "state": state,
                "total": total,
                "updatedAt": updated_at,
            }
        )
        if ended_at is not UNSET:
            field_dict["endedAt"] = ended_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_view_source import BatchViewSource

        d = dict(src_dict)
        agent = d.pop("agent")

        batch_id = d.pop("batchId")

        canceled = d.pop("canceled")

        completed = d.pop("completed")

        concurrency = d.pop("concurrency")

        created_at = d.pop("createdAt")

        failed = d.pop("failed")

        grants = cast(list[str], d.pop("grants"))

        max_failures = d.pop("maxFailures")

        source = BatchViewSource.from_dict(d.pop("source"))

        state = d.pop("state")

        total = d.pop("total")

        updated_at = d.pop("updatedAt")

        def _parse_ended_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        ended_at = _parse_ended_at(d.pop("endedAt", UNSET))

        batch_view = cls(
            agent=agent,
            batch_id=batch_id,
            canceled=canceled,
            completed=completed,
            concurrency=concurrency,
            created_at=created_at,
            failed=failed,
            grants=grants,
            max_failures=max_failures,
            source=source,
            state=state,
            total=total,
            updated_at=updated_at,
            ended_at=ended_at,
        )

        batch_view.additional_properties = d
        return batch_view

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
