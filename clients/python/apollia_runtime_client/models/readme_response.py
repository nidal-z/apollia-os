from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ReadmeResponse")


@_attrs_define
class ReadmeResponse:
    """A README, as text.

    Attributes:
        author (str): Its owner.
        repo (str): The repository, `owner/name`.
        source_url (str): The page of the repository.
        text (str): The text.
        license_ (None | str | Unset): The licence its front matter declares.
    """

    author: str
    repo: str
    source_url: str
    text: str
    license_: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        author = self.author

        repo = self.repo

        source_url = self.source_url

        text = self.text

        license_: None | str | Unset
        if isinstance(self.license_, Unset):
            license_ = UNSET
        else:
            license_ = self.license_

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "author": author,
                "repo": repo,
                "source_url": source_url,
                "text": text,
            }
        )
        if license_ is not UNSET:
            field_dict["license"] = license_

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        author = d.pop("author")

        repo = d.pop("repo")

        source_url = d.pop("source_url")

        text = d.pop("text")

        def _parse_license_(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        license_ = _parse_license_(d.pop("license", UNSET))

        readme_response = cls(
            author=author,
            repo=repo,
            source_url=source_url,
            text=text,
            license_=license_,
        )

        readme_response.additional_properties = d
        return readme_response

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
