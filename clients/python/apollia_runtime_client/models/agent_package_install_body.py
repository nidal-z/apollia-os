from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.agent_package_trigger_config import AgentPackageTriggerConfig


T = TypeVar("T", bound="AgentPackageInstallBody")


@_attrs_define
class AgentPackageInstallBody:
    """Body of `POST /api/v1/agent-packages/installed`.

    Attributes:
        path (str): The package folder on this machine, the one holding `agent.toml`.
        deps_confirmed (bool | Unset): The user accepted the Python dependencies the package installs.
        trigger_configs (list[AgentPackageTriggerConfig] | Unset): What the user gave each trigger, a webhook secret for
            now.
    """

    path: str
    deps_confirmed: bool | Unset = UNSET
    trigger_configs: list[AgentPackageTriggerConfig] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        path = self.path

        deps_confirmed = self.deps_confirmed

        trigger_configs: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.trigger_configs, Unset):
            trigger_configs = []
            for trigger_configs_item_data in self.trigger_configs:
                trigger_configs_item = trigger_configs_item_data.to_dict()
                trigger_configs.append(trigger_configs_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "path": path,
            }
        )
        if deps_confirmed is not UNSET:
            field_dict["deps_confirmed"] = deps_confirmed
        if trigger_configs is not UNSET:
            field_dict["trigger_configs"] = trigger_configs

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_package_trigger_config import AgentPackageTriggerConfig

        d = dict(src_dict)
        path = d.pop("path")

        deps_confirmed = d.pop("deps_confirmed", UNSET)

        _trigger_configs = d.pop("trigger_configs", UNSET)
        trigger_configs: list[AgentPackageTriggerConfig] | Unset = UNSET
        if _trigger_configs is not UNSET:
            trigger_configs = []
            for trigger_configs_item_data in _trigger_configs:
                trigger_configs_item = AgentPackageTriggerConfig.from_dict(
                    trigger_configs_item_data
                )

                trigger_configs.append(trigger_configs_item)

        agent_package_install_body = cls(
            path=path,
            deps_confirmed=deps_confirmed,
            trigger_configs=trigger_configs,
        )

        agent_package_install_body.additional_properties = d
        return agent_package_install_body

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
