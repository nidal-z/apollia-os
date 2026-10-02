from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.package_notice_type_0 import PackageNoticeType0
    from ..models.package_notice_type_1 import PackageNoticeType1
    from ..models.package_notice_type_2 import PackageNoticeType2
    from ..models.package_notice_type_3 import PackageNoticeType3
    from ..models.signature_preview import SignaturePreview


T = TypeVar("T", bound="InstallReport")


@_attrs_define
class InstallReport:
    """What was installed.

    Attributes:
        agents (list[str]): Agents installed.
        mcp_servers_added (list[str]): MCP servers added.
        name (str): Package identifier.
        notices (list[PackageNoticeType0 | PackageNoticeType1 | PackageNoticeType2 | PackageNoticeType3]): What the user
            should know, as the preview said.
        signature (SignaturePreview): The publisher and the signature, as the preview shows them.
        tools (list[str]): Python tools installed.
        version (str): Installed version.
        workflows (list[str]): Workflows installed.
        previous_version (None | str | Unset): Version it replaced, for an update.
    """

    agents: list[str]
    mcp_servers_added: list[str]
    name: str
    notices: list[PackageNoticeType0 | PackageNoticeType1 | PackageNoticeType2 | PackageNoticeType3]
    signature: SignaturePreview
    tools: list[str]
    version: str
    workflows: list[str]
    previous_version: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.package_notice_type_0 import PackageNoticeType0
        from ..models.package_notice_type_1 import PackageNoticeType1
        from ..models.package_notice_type_2 import PackageNoticeType2

        agents = self.agents

        mcp_servers_added = self.mcp_servers_added

        name = self.name

        notices = []
        for notices_item_data in self.notices:
            notices_item: dict[str, Any]
            if isinstance(notices_item_data, PackageNoticeType0):
                notices_item = notices_item_data.to_dict()
            elif isinstance(notices_item_data, PackageNoticeType1):
                notices_item = notices_item_data.to_dict()
            elif isinstance(notices_item_data, PackageNoticeType2):
                notices_item = notices_item_data.to_dict()
            else:
                notices_item = notices_item_data.to_dict()

            notices.append(notices_item)

        signature = self.signature.to_dict()

        tools = self.tools

        version = self.version

        workflows = self.workflows

        previous_version: None | str | Unset
        if isinstance(self.previous_version, Unset):
            previous_version = UNSET
        else:
            previous_version = self.previous_version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "mcpServersAdded": mcp_servers_added,
                "name": name,
                "notices": notices,
                "signature": signature,
                "tools": tools,
                "version": version,
                "workflows": workflows,
            }
        )
        if previous_version is not UNSET:
            field_dict["previousVersion"] = previous_version

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.package_notice_type_0 import PackageNoticeType0
        from ..models.package_notice_type_1 import PackageNoticeType1
        from ..models.package_notice_type_2 import PackageNoticeType2
        from ..models.package_notice_type_3 import PackageNoticeType3
        from ..models.signature_preview import SignaturePreview

        d = dict(src_dict)
        agents = cast(list[str], d.pop("agents"))

        mcp_servers_added = cast(list[str], d.pop("mcpServersAdded"))

        name = d.pop("name")

        notices = []
        _notices = d.pop("notices")
        for notices_item_data in _notices:

            def _parse_notices_item(
                data: object,
            ) -> PackageNoticeType0 | PackageNoticeType1 | PackageNoticeType2 | PackageNoticeType3:
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_package_notice_type_0 = PackageNoticeType0.from_dict(data)

                    return componentsschemas_package_notice_type_0
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_package_notice_type_1 = PackageNoticeType1.from_dict(data)

                    return componentsschemas_package_notice_type_1
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_package_notice_type_2 = PackageNoticeType2.from_dict(data)

                    return componentsschemas_package_notice_type_2
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_package_notice_type_3 = PackageNoticeType3.from_dict(data)

                return componentsschemas_package_notice_type_3

            notices_item = _parse_notices_item(notices_item_data)

            notices.append(notices_item)

        signature = SignaturePreview.from_dict(d.pop("signature"))

        tools = cast(list[str], d.pop("tools"))

        version = d.pop("version")

        workflows = cast(list[str], d.pop("workflows"))

        def _parse_previous_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        previous_version = _parse_previous_version(d.pop("previousVersion", UNSET))

        install_report = cls(
            agents=agents,
            mcp_servers_added=mcp_servers_added,
            name=name,
            notices=notices,
            signature=signature,
            tools=tools,
            version=version,
            workflows=workflows,
            previous_version=previous_version,
        )

        install_report.additional_properties = d
        return install_report

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
