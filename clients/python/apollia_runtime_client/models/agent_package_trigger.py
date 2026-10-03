from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AgentPackageTrigger")


@_attrs_define
class AgentPackageTrigger:
    """A trigger a package declares, as the preview shows it.

    Attributes:
        agent (str): The agent it starts.
        enabled (bool): Whether it starts enabled.
        id (str): Trigger identifier.
        needs_config (bool): A webhook without its secret: the user gives one at install.
        source_type (str): `cron`, `interval`, `oneshot`, `file_watch` or `webhook`.
        every (None | str | Unset): Interval (interval triggers).
        path (None | str | Unset): Watched path (file_watch triggers).
        schedule (None | str | Unset): Cron expression (cron triggers).
    """

    agent: str
    enabled: bool
    id: str
    needs_config: bool
    source_type: str
    every: None | str | Unset = UNSET
    path: None | str | Unset = UNSET
    schedule: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent = self.agent

        enabled = self.enabled

        id = self.id

        needs_config = self.needs_config

        source_type = self.source_type

        every: None | str | Unset
        if isinstance(self.every, Unset):
            every = UNSET
        else:
            every = self.every

        path: None | str | Unset
        if isinstance(self.path, Unset):
            path = UNSET
        else:
            path = self.path

        schedule: None | str | Unset
        if isinstance(self.schedule, Unset):
            schedule = UNSET
        else:
            schedule = self.schedule

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent": agent,
                "enabled": enabled,
                "id": id,
                "needs_config": needs_config,
                "source_type": source_type,
            }
        )
        if every is not UNSET:
            field_dict["every"] = every
        if path is not UNSET:
            field_dict["path"] = path
        if schedule is not UNSET:
            field_dict["schedule"] = schedule

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent = d.pop("agent")

        enabled = d.pop("enabled")

        id = d.pop("id")

        needs_config = d.pop("needs_config")

        source_type = d.pop("source_type")

        def _parse_every(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        every = _parse_every(d.pop("every", UNSET))

        def _parse_path(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        path = _parse_path(d.pop("path", UNSET))

        def _parse_schedule(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        schedule = _parse_schedule(d.pop("schedule", UNSET))

        agent_package_trigger = cls(
            agent=agent,
            enabled=enabled,
            id=id,
            needs_config=needs_config,
            source_type=source_type,
            every=every,
            path=path,
            schedule=schedule,
        )

        agent_package_trigger.additional_properties = d
        return agent_package_trigger

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
