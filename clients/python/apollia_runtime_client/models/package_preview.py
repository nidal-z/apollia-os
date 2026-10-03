from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.agent_preview import AgentPreview
    from ..models.brought_server_preview import BroughtServerPreview
    from ..models.install_refusal_type_0 import InstallRefusalType0
    from ..models.install_refusal_type_1 import InstallRefusalType1
    from ..models.install_refusal_type_2 import InstallRefusalType2
    from ..models.install_refusal_type_3 import InstallRefusalType3
    from ..models.install_refusal_type_4 import InstallRefusalType4
    from ..models.item_preview import ItemPreview
    from ..models.package_notice_type_0 import PackageNoticeType0
    from ..models.package_notice_type_1 import PackageNoticeType1
    from ..models.package_notice_type_2 import PackageNoticeType2
    from ..models.package_notice_type_3 import PackageNoticeType3
    from ..models.required_connection_preview import RequiredConnectionPreview
    from ..models.signature_preview import SignaturePreview
    from ..models.trigger_preview import TriggerPreview
    from ..models.update_diff import UpdateDiff
    from ..models.workflow_preview import WorkflowPreview


T = TypeVar("T", bound="PackagePreview")


@_attrs_define
class PackagePreview:
    """Everything the user sees before installing a package.

    Attributes:
        agents (list[AgentPreview]): Agents.
        description (str): What the package delivers.
        mcp_servers (list[BroughtServerPreview]): MCP servers the package brings.
        name (str): Package identifier.
        notices (list[PackageNoticeType0 | PackageNoticeType1 | PackageNoticeType2 | PackageNoticeType3]): What the user
            should know.
        pip (list[str]): Python requirements installed in the package environment.
        requires (list[RequiredConnectionPreview]): Connections the package requires.
        sdk (str): SDK requirement the package was built against.
        signature (SignaturePreview): The publisher and the signature, as the preview shows them.
        signature_required (bool): Governance requires a signed package.
        tools (list[ItemPreview]): Python tools.
        triggers (list[TriggerPreview]): Triggers the package proposes.
        version (str): Incoming version.
        workflows (list[WorkflowPreview]): Workflows.
        installed_version (None | str | Unset): Installed version, when the package is installed.
        refusal (InstallRefusalType0 | InstallRefusalType1 | InstallRefusalType2 | InstallRefusalType3 |
            InstallRefusalType4 | None | Unset):
        update (None | Unset | UpdateDiff):
    """

    agents: list[AgentPreview]
    description: str
    mcp_servers: list[BroughtServerPreview]
    name: str
    notices: list[PackageNoticeType0 | PackageNoticeType1 | PackageNoticeType2 | PackageNoticeType3]
    pip: list[str]
    requires: list[RequiredConnectionPreview]
    sdk: str
    signature: SignaturePreview
    signature_required: bool
    tools: list[ItemPreview]
    triggers: list[TriggerPreview]
    version: str
    workflows: list[WorkflowPreview]
    installed_version: None | str | Unset = UNSET
    refusal: (
        InstallRefusalType0
        | InstallRefusalType1
        | InstallRefusalType2
        | InstallRefusalType3
        | InstallRefusalType4
        | None
        | Unset
    ) = UNSET
    update: None | Unset | UpdateDiff = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.install_refusal_type_0 import InstallRefusalType0
        from ..models.install_refusal_type_1 import InstallRefusalType1
        from ..models.install_refusal_type_2 import InstallRefusalType2
        from ..models.install_refusal_type_3 import InstallRefusalType3
        from ..models.install_refusal_type_4 import InstallRefusalType4
        from ..models.package_notice_type_0 import PackageNoticeType0
        from ..models.package_notice_type_1 import PackageNoticeType1
        from ..models.package_notice_type_2 import PackageNoticeType2
        from ..models.update_diff import UpdateDiff

        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        description = self.description

        mcp_servers = []
        for mcp_servers_item_data in self.mcp_servers:
            mcp_servers_item = mcp_servers_item_data.to_dict()
            mcp_servers.append(mcp_servers_item)

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

        pip = self.pip

        requires = []
        for requires_item_data in self.requires:
            requires_item = requires_item_data.to_dict()
            requires.append(requires_item)

        sdk = self.sdk

        signature = self.signature.to_dict()

        signature_required = self.signature_required

        tools = []
        for tools_item_data in self.tools:
            tools_item = tools_item_data.to_dict()
            tools.append(tools_item)

        triggers = []
        for triggers_item_data in self.triggers:
            triggers_item = triggers_item_data.to_dict()
            triggers.append(triggers_item)

        version = self.version

        workflows = []
        for workflows_item_data in self.workflows:
            workflows_item = workflows_item_data.to_dict()
            workflows.append(workflows_item)

        installed_version: None | str | Unset
        if isinstance(self.installed_version, Unset):
            installed_version = UNSET
        else:
            installed_version = self.installed_version

        refusal: dict[str, Any] | None | Unset
        if isinstance(self.refusal, Unset):
            refusal = UNSET
        elif isinstance(self.refusal, InstallRefusalType0):
            refusal = self.refusal.to_dict()
        elif isinstance(self.refusal, InstallRefusalType1):
            refusal = self.refusal.to_dict()
        elif isinstance(self.refusal, InstallRefusalType2):
            refusal = self.refusal.to_dict()
        elif isinstance(self.refusal, InstallRefusalType3):
            refusal = self.refusal.to_dict()
        elif isinstance(self.refusal, InstallRefusalType4):
            refusal = self.refusal.to_dict()
        else:
            refusal = self.refusal

        update: dict[str, Any] | None | Unset
        if isinstance(self.update, Unset):
            update = UNSET
        elif isinstance(self.update, UpdateDiff):
            update = self.update.to_dict()
        else:
            update = self.update

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "description": description,
                "mcpServers": mcp_servers,
                "name": name,
                "notices": notices,
                "pip": pip,
                "requires": requires,
                "sdk": sdk,
                "signature": signature,
                "signatureRequired": signature_required,
                "tools": tools,
                "triggers": triggers,
                "version": version,
                "workflows": workflows,
            }
        )
        if installed_version is not UNSET:
            field_dict["installedVersion"] = installed_version
        if refusal is not UNSET:
            field_dict["refusal"] = refusal
        if update is not UNSET:
            field_dict["update"] = update

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_preview import AgentPreview
        from ..models.brought_server_preview import BroughtServerPreview
        from ..models.install_refusal_type_0 import InstallRefusalType0
        from ..models.install_refusal_type_1 import InstallRefusalType1
        from ..models.install_refusal_type_2 import InstallRefusalType2
        from ..models.install_refusal_type_3 import InstallRefusalType3
        from ..models.install_refusal_type_4 import InstallRefusalType4
        from ..models.item_preview import ItemPreview
        from ..models.package_notice_type_0 import PackageNoticeType0
        from ..models.package_notice_type_1 import PackageNoticeType1
        from ..models.package_notice_type_2 import PackageNoticeType2
        from ..models.package_notice_type_3 import PackageNoticeType3
        from ..models.required_connection_preview import RequiredConnectionPreview
        from ..models.signature_preview import SignaturePreview
        from ..models.trigger_preview import TriggerPreview
        from ..models.update_diff import UpdateDiff
        from ..models.workflow_preview import WorkflowPreview

        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = AgentPreview.from_dict(agents_item_data)

            agents.append(agents_item)

        description = d.pop("description")

        mcp_servers = []
        _mcp_servers = d.pop("mcpServers")
        for mcp_servers_item_data in _mcp_servers:
            mcp_servers_item = BroughtServerPreview.from_dict(mcp_servers_item_data)

            mcp_servers.append(mcp_servers_item)

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

        pip = cast(list[str], d.pop("pip"))

        requires = []
        _requires = d.pop("requires")
        for requires_item_data in _requires:
            requires_item = RequiredConnectionPreview.from_dict(requires_item_data)

            requires.append(requires_item)

        sdk = d.pop("sdk")

        signature = SignaturePreview.from_dict(d.pop("signature"))

        signature_required = d.pop("signatureRequired")

        tools = []
        _tools = d.pop("tools")
        for tools_item_data in _tools:
            tools_item = ItemPreview.from_dict(tools_item_data)

            tools.append(tools_item)

        triggers = []
        _triggers = d.pop("triggers")
        for triggers_item_data in _triggers:
            triggers_item = TriggerPreview.from_dict(triggers_item_data)

            triggers.append(triggers_item)

        version = d.pop("version")

        workflows = []
        _workflows = d.pop("workflows")
        for workflows_item_data in _workflows:
            workflows_item = WorkflowPreview.from_dict(workflows_item_data)

            workflows.append(workflows_item)

        def _parse_installed_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        installed_version = _parse_installed_version(d.pop("installedVersion", UNSET))

        def _parse_refusal(
            data: object,
        ) -> (
            InstallRefusalType0
            | InstallRefusalType1
            | InstallRefusalType2
            | InstallRefusalType3
            | InstallRefusalType4
            | None
            | Unset
        ):
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_install_refusal_type_0 = InstallRefusalType0.from_dict(data)

                return componentsschemas_install_refusal_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_install_refusal_type_1 = InstallRefusalType1.from_dict(data)

                return componentsschemas_install_refusal_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_install_refusal_type_2 = InstallRefusalType2.from_dict(data)

                return componentsschemas_install_refusal_type_2
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_install_refusal_type_3 = InstallRefusalType3.from_dict(data)

                return componentsschemas_install_refusal_type_3
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_install_refusal_type_4 = InstallRefusalType4.from_dict(data)

                return componentsschemas_install_refusal_type_4
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(
                InstallRefusalType0
                | InstallRefusalType1
                | InstallRefusalType2
                | InstallRefusalType3
                | InstallRefusalType4
                | None
                | Unset,
                data,
            )

        refusal = _parse_refusal(d.pop("refusal", UNSET))

        def _parse_update(data: object) -> None | Unset | UpdateDiff:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                update_type_1 = UpdateDiff.from_dict(data)

                return update_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | Unset | UpdateDiff, data)

        update = _parse_update(d.pop("update", UNSET))

        package_preview = cls(
            agents=agents,
            description=description,
            mcp_servers=mcp_servers,
            name=name,
            notices=notices,
            pip=pip,
            requires=requires,
            sdk=sdk,
            signature=signature,
            signature_required=signature_required,
            tools=tools,
            triggers=triggers,
            version=version,
            workflows=workflows,
            installed_version=installed_version,
            refusal=refusal,
            update=update,
        )

        package_preview.additional_properties = d
        return package_preview

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
