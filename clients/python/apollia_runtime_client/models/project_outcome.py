from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.memory_merge import MemoryMerge


T = TypeVar("T", bound="ProjectOutcome")


@_attrs_define
class ProjectOutcome:
    """What the migration did for one project.

    Attributes:
        agents_checked (list[str] | Unset): Agents its sources are checked for.
        document_sources (list[str] | Unset): Identifiers of the sources made of its documents.
        folder_connection (None | str | Unset): Identifier of the folder connection made of its working folder.
        id (str | Unset): Identifier of the project. Default: ''.
        instructions_source (None | str | Unset): Identifier of the source made of its instructions, if it had any.
        memory (list[MemoryMerge] | Unset): Its memory spaces, merged.
        name (str | Unset): Name of the project, which is also the group of its sources. Default: ''.
    """

    agents_checked: list[str] | Unset = UNSET
    document_sources: list[str] | Unset = UNSET
    folder_connection: None | str | Unset = UNSET
    id: str | Unset = ""
    instructions_source: None | str | Unset = UNSET
    memory: list[MemoryMerge] | Unset = UNSET
    name: str | Unset = ""
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents_checked: list[str] | Unset = UNSET
        if not isinstance(self.agents_checked, Unset):
            agents_checked = self.agents_checked

        document_sources: list[str] | Unset = UNSET
        if not isinstance(self.document_sources, Unset):
            document_sources = self.document_sources

        folder_connection: None | str | Unset
        if isinstance(self.folder_connection, Unset):
            folder_connection = UNSET
        else:
            folder_connection = self.folder_connection

        id = self.id

        instructions_source: None | str | Unset
        if isinstance(self.instructions_source, Unset):
            instructions_source = UNSET
        else:
            instructions_source = self.instructions_source

        memory: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.memory, Unset):
            memory = []
            for memory_item_data in self.memory:
                memory_item = memory_item_data.to_dict()
                memory.append(memory_item)

        name = self.name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if agents_checked is not UNSET:
            field_dict["agentsChecked"] = agents_checked
        if document_sources is not UNSET:
            field_dict["documentSources"] = document_sources
        if folder_connection is not UNSET:
            field_dict["folderConnection"] = folder_connection
        if id is not UNSET:
            field_dict["id"] = id
        if instructions_source is not UNSET:
            field_dict["instructionsSource"] = instructions_source
        if memory is not UNSET:
            field_dict["memory"] = memory
        if name is not UNSET:
            field_dict["name"] = name

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.memory_merge import MemoryMerge

        d = dict(src_dict)
        agents_checked = cast(list[str], d.pop("agentsChecked", UNSET))

        document_sources = cast(list[str], d.pop("documentSources", UNSET))

        def _parse_folder_connection(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        folder_connection = _parse_folder_connection(d.pop("folderConnection", UNSET))

        id = d.pop("id", UNSET)

        def _parse_instructions_source(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        instructions_source = _parse_instructions_source(d.pop("instructionsSource", UNSET))

        _memory = d.pop("memory", UNSET)
        memory: list[MemoryMerge] | Unset = UNSET
        if _memory is not UNSET:
            memory = []
            for memory_item_data in _memory:
                memory_item = MemoryMerge.from_dict(memory_item_data)

                memory.append(memory_item)

        name = d.pop("name", UNSET)

        project_outcome = cls(
            agents_checked=agents_checked,
            document_sources=document_sources,
            folder_connection=folder_connection,
            id=id,
            instructions_source=instructions_source,
            memory=memory,
            name=name,
        )

        project_outcome.additional_properties = d
        return project_outcome

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
