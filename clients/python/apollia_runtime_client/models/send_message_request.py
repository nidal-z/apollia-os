from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.document_attachment import DocumentAttachment
    from ..models.image_attachment import ImageAttachment


T = TypeVar("T", bound="SendMessageRequest")


@_attrs_define
class SendMessageRequest:
    """Request body for `POST /api/v1/sessions/:id/messages`.

    Attributes:
        content (str): Text content of the user message.
        documents (list[DocumentAttachment] | Unset): Documents sent with the message (PDF, Word, PowerPoint, workbook,
            CSV):
            the runtime extracts their text on reception. At most four, each at
            most 20 MiB once decoded.
        images (list[ImageAttachment] | Unset): Images sent after the text, for a model that reads images; at most
            eight, each at most 10 MiB once decoded.
    """

    content: str
    documents: list[DocumentAttachment] | Unset = UNSET
    images: list[ImageAttachment] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        content = self.content

        documents: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.documents, Unset):
            documents = []
            for documents_item_data in self.documents:
                documents_item = documents_item_data.to_dict()
                documents.append(documents_item)

        images: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.images, Unset):
            images = []
            for images_item_data in self.images:
                images_item = images_item_data.to_dict()
                images.append(images_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "content": content,
            }
        )
        if documents is not UNSET:
            field_dict["documents"] = documents
        if images is not UNSET:
            field_dict["images"] = images

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.document_attachment import DocumentAttachment
        from ..models.image_attachment import ImageAttachment

        d = dict(src_dict)
        content = d.pop("content")

        _documents = d.pop("documents", UNSET)
        documents: list[DocumentAttachment] | Unset = UNSET
        if _documents is not UNSET:
            documents = []
            for documents_item_data in _documents:
                documents_item = DocumentAttachment.from_dict(documents_item_data)

                documents.append(documents_item)

        _images = d.pop("images", UNSET)
        images: list[ImageAttachment] | Unset = UNSET
        if _images is not UNSET:
            images = []
            for images_item_data in _images:
                images_item = ImageAttachment.from_dict(images_item_data)

                images.append(images_item)

        send_message_request = cls(
            content=content,
            documents=documents,
            images=images,
        )

        send_message_request.additional_properties = d
        return send_message_request

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
