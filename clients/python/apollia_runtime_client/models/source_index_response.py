from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.index_state_dto import IndexStateDto

if TYPE_CHECKING:
    from ..models.skipped_file_response import SkippedFileResponse
    from ..models.unread_page_response import UnreadPageResponse


T = TypeVar("T", bound="SourceIndexResponse")


@_attrs_define
class SourceIndexResponse:
    """Where the index of a source stands.

    Attributes:
        chunks (int): Passages the index holds.
        documents (int): Documents the index holds.
        id (str): Source identifier.
        index_state (IndexStateDto): Where the index of a source stands.
        skipped (list[SkippedFileResponse]): Files left out of the index.
        unread_pages (list[UnreadPageResponse]): Pages of indexed documents that hold no text: those waiting for
            optical character recognition, and those it cannot read.
    """

    chunks: int
    documents: int
    id: str
    index_state: IndexStateDto
    skipped: list[SkippedFileResponse]
    unread_pages: list[UnreadPageResponse]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        chunks = self.chunks

        documents = self.documents

        id = self.id

        index_state = self.index_state.value

        skipped = []
        for skipped_item_data in self.skipped:
            skipped_item = skipped_item_data.to_dict()
            skipped.append(skipped_item)

        unread_pages = []
        for unread_pages_item_data in self.unread_pages:
            unread_pages_item = unread_pages_item_data.to_dict()
            unread_pages.append(unread_pages_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "chunks": chunks,
                "documents": documents,
                "id": id,
                "indexState": index_state,
                "skipped": skipped,
                "unreadPages": unread_pages,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.skipped_file_response import SkippedFileResponse
        from ..models.unread_page_response import UnreadPageResponse

        d = dict(src_dict)
        chunks = d.pop("chunks")

        documents = d.pop("documents")

        id = d.pop("id")

        index_state = IndexStateDto(d.pop("indexState"))

        skipped = []
        _skipped = d.pop("skipped")
        for skipped_item_data in _skipped:
            skipped_item = SkippedFileResponse.from_dict(skipped_item_data)

            skipped.append(skipped_item)

        unread_pages = []
        _unread_pages = d.pop("unreadPages")
        for unread_pages_item_data in _unread_pages:
            unread_pages_item = UnreadPageResponse.from_dict(unread_pages_item_data)

            unread_pages.append(unread_pages_item)

        source_index_response = cls(
            chunks=chunks,
            documents=documents,
            id=id,
            index_state=index_state,
            skipped=skipped,
            unread_pages=unread_pages,
        )

        source_index_response.additional_properties = d
        return source_index_response

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
