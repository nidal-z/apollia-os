from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.batch_task import BatchTask
    from ..models.source_descriptor import SourceDescriptor


T = TypeVar("T", bound="BatchLaunch")


@_attrs_define
class BatchLaunch:
    """What a batch is launched with.

    Attributes:
        agent (str): The agent every item is run by.
        source (SourceDescriptor): The source a batch reads its items from.
        concurrency (int | None | Unset): How many items run at once, 1 to 16; 1 when absent.
        grants (list[str] | Unset): The sensitive accesses granted for the duration of the batch, by
            their id as `GET /api/v1/batches/accesses` lists them, and
            `source:write` when the results go into the original file.
        max_failures (int | None | Unset): How many failed items the batch tolerates; 10 when absent.
        task (BatchTask | None | Unset):
    """

    agent: str
    source: SourceDescriptor
    concurrency: int | None | Unset = UNSET
    grants: list[str] | Unset = UNSET
    max_failures: int | None | Unset = UNSET
    task: BatchTask | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.batch_task import BatchTask

        agent = self.agent

        source = self.source.to_dict()

        concurrency: int | None | Unset
        if isinstance(self.concurrency, Unset):
            concurrency = UNSET
        else:
            concurrency = self.concurrency

        grants: list[str] | Unset = UNSET
        if not isinstance(self.grants, Unset):
            grants = self.grants

        max_failures: int | None | Unset
        if isinstance(self.max_failures, Unset):
            max_failures = UNSET
        else:
            max_failures = self.max_failures

        task: dict[str, Any] | None | Unset
        if isinstance(self.task, Unset):
            task = UNSET
        elif isinstance(self.task, BatchTask):
            task = self.task.to_dict()
        else:
            task = self.task

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent": agent,
                "source": source,
            }
        )
        if concurrency is not UNSET:
            field_dict["concurrency"] = concurrency
        if grants is not UNSET:
            field_dict["grants"] = grants
        if max_failures is not UNSET:
            field_dict["maxFailures"] = max_failures
        if task is not UNSET:
            field_dict["task"] = task

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_task import BatchTask
        from ..models.source_descriptor import SourceDescriptor

        d = dict(src_dict)
        agent = d.pop("agent")

        source = SourceDescriptor.from_dict(d.pop("source"))

        def _parse_concurrency(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        concurrency = _parse_concurrency(d.pop("concurrency", UNSET))

        grants = cast(list[str], d.pop("grants", UNSET))

        def _parse_max_failures(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        max_failures = _parse_max_failures(d.pop("maxFailures", UNSET))

        def _parse_task(data: object) -> BatchTask | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                task_type_1 = BatchTask.from_dict(data)

                return task_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(BatchTask | None | Unset, data)

        task = _parse_task(d.pop("task", UNSET))

        batch_launch = cls(
            agent=agent,
            source=source,
            concurrency=concurrency,
            grants=grants,
            max_failures=max_failures,
            task=task,
        )

        batch_launch.additional_properties = d
        return batch_launch

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
