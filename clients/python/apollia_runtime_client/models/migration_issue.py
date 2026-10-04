from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.issue_item import IssueItem
from ..models.issue_reason import IssueReason
from ..types import UNSET, Unset

T = TypeVar("T", bound="MigrationIssue")


@_attrs_define
class MigrationIssue:
    """One item the migration did not carry over.

    Attributes:
        blocking (bool | Unset): Whether the issue keeps the migration partial. A missing document is
            listed and does not: running again would not bring it back. Default: False.
        detail (None | str | Unset): The underlying error, when there is one.
        item (IssueItem | Unset): What an issue is about. Default: IssueItem.STORE.
        project_id (None | str | Unset): Identifier of the project, if the issue belongs to one.
        project_name (None | str | Unset): Name of the project, if the issue belongs to one.
        reason (IssueReason | Unset): Why an item was not carried over. Default: IssueReason.FAILED.
        subject (str | Unset): The path, agent or space concerned. Default: ''.
    """

    blocking: bool | Unset = False
    detail: None | str | Unset = UNSET
    item: IssueItem | Unset = IssueItem.STORE
    project_id: None | str | Unset = UNSET
    project_name: None | str | Unset = UNSET
    reason: IssueReason | Unset = IssueReason.FAILED
    subject: str | Unset = ""
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        blocking = self.blocking

        detail: None | str | Unset
        if isinstance(self.detail, Unset):
            detail = UNSET
        else:
            detail = self.detail

        item: str | Unset = UNSET
        if not isinstance(self.item, Unset):
            item = self.item.value

        project_id: None | str | Unset
        if isinstance(self.project_id, Unset):
            project_id = UNSET
        else:
            project_id = self.project_id

        project_name: None | str | Unset
        if isinstance(self.project_name, Unset):
            project_name = UNSET
        else:
            project_name = self.project_name

        reason: str | Unset = UNSET
        if not isinstance(self.reason, Unset):
            reason = self.reason.value

        subject = self.subject

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if blocking is not UNSET:
            field_dict["blocking"] = blocking
        if detail is not UNSET:
            field_dict["detail"] = detail
        if item is not UNSET:
            field_dict["item"] = item
        if project_id is not UNSET:
            field_dict["projectId"] = project_id
        if project_name is not UNSET:
            field_dict["projectName"] = project_name
        if reason is not UNSET:
            field_dict["reason"] = reason
        if subject is not UNSET:
            field_dict["subject"] = subject

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        blocking = d.pop("blocking", UNSET)

        def _parse_detail(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        detail = _parse_detail(d.pop("detail", UNSET))

        _item = d.pop("item", UNSET)
        item: IssueItem | Unset
        if isinstance(_item, Unset):
            item = UNSET
        else:
            item = IssueItem(_item)

        def _parse_project_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        project_id = _parse_project_id(d.pop("projectId", UNSET))

        def _parse_project_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        project_name = _parse_project_name(d.pop("projectName", UNSET))

        _reason = d.pop("reason", UNSET)
        reason: IssueReason | Unset
        if isinstance(_reason, Unset):
            reason = UNSET
        else:
            reason = IssueReason(_reason)

        subject = d.pop("subject", UNSET)

        migration_issue = cls(
            blocking=blocking,
            detail=detail,
            item=item,
            project_id=project_id,
            project_name=project_name,
            reason=reason,
            subject=subject,
        )

        migration_issue.additional_properties = d
        return migration_issue

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
