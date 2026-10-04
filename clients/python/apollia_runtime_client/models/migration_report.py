from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.migration_status import MigrationStatus
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.migration_issue import MigrationIssue
    from ..models.project_outcome import ProjectOutcome


T = TypeVar("T", bound="MigrationReport")


@_attrs_define
class MigrationReport:
    """The report of a run of the migration.

    Attributes:
        conversations_detached (int | Unset): Conversations detached from a project by this run. Default: 0.
        format_version (int | Unset): Version of this format. Default: 0.
        issues (list[MigrationIssue] | Unset): What was not carried over.
        projects (list[ProjectOutcome] | Unset): One entry per project found.
        ran_at (str | Unset): When the run ended, RFC 3339 in UTC. Default: ''.
        status (MigrationStatus | Unset): Where a run of the migration ended. Default: MigrationStatus.PARTIAL.
    """

    conversations_detached: int | Unset = 0
    format_version: int | Unset = 0
    issues: list[MigrationIssue] | Unset = UNSET
    projects: list[ProjectOutcome] | Unset = UNSET
    ran_at: str | Unset = ""
    status: MigrationStatus | Unset = MigrationStatus.PARTIAL
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        conversations_detached = self.conversations_detached

        format_version = self.format_version

        issues: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.issues, Unset):
            issues = []
            for issues_item_data in self.issues:
                issues_item = issues_item_data.to_dict()
                issues.append(issues_item)

        projects: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.projects, Unset):
            projects = []
            for projects_item_data in self.projects:
                projects_item = projects_item_data.to_dict()
                projects.append(projects_item)

        ran_at = self.ran_at

        status: str | Unset = UNSET
        if not isinstance(self.status, Unset):
            status = self.status.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if conversations_detached is not UNSET:
            field_dict["conversationsDetached"] = conversations_detached
        if format_version is not UNSET:
            field_dict["formatVersion"] = format_version
        if issues is not UNSET:
            field_dict["issues"] = issues
        if projects is not UNSET:
            field_dict["projects"] = projects
        if ran_at is not UNSET:
            field_dict["ranAt"] = ran_at
        if status is not UNSET:
            field_dict["status"] = status

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.migration_issue import MigrationIssue
        from ..models.project_outcome import ProjectOutcome

        d = dict(src_dict)
        conversations_detached = d.pop("conversationsDetached", UNSET)

        format_version = d.pop("formatVersion", UNSET)

        _issues = d.pop("issues", UNSET)
        issues: list[MigrationIssue] | Unset = UNSET
        if _issues is not UNSET:
            issues = []
            for issues_item_data in _issues:
                issues_item = MigrationIssue.from_dict(issues_item_data)

                issues.append(issues_item)

        _projects = d.pop("projects", UNSET)
        projects: list[ProjectOutcome] | Unset = UNSET
        if _projects is not UNSET:
            projects = []
            for projects_item_data in _projects:
                projects_item = ProjectOutcome.from_dict(projects_item_data)

                projects.append(projects_item)

        ran_at = d.pop("ranAt", UNSET)

        _status = d.pop("status", UNSET)
        status: MigrationStatus | Unset
        if isinstance(_status, Unset):
            status = UNSET
        else:
            status = MigrationStatus(_status)

        migration_report = cls(
            conversations_detached=conversations_detached,
            format_version=format_version,
            issues=issues,
            projects=projects,
            ran_at=ran_at,
            status=status,
        )

        migration_report.additional_properties = d
        return migration_report

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
