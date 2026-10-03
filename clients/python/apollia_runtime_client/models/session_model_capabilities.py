from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.reasoning_profile_dto import ReasoningProfileDto
from ..types import UNSET, Unset

T = TypeVar("T", bound="SessionModelCapabilities")


@_attrs_define
class SessionModelCapabilities:
    """Answer of `GET /api/v1/sessions/{id}/model-capabilities`.

    Attributes:
        image_formats (list[str]): The image formats its engine decodes, empty without vision.
        local (bool): Whether a local engine serves the model.
        vision (bool): Whether the model reads images.
        model (None | str | Unset): The backend or model the conversation runs on, the default model of
            the machine when it names none; `null` when no model is configured.
        reasoning_profile (None | ReasoningProfileDto | Unset):
    """

    image_formats: list[str]
    local: bool
    vision: bool
    model: None | str | Unset = UNSET
    reasoning_profile: None | ReasoningProfileDto | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        image_formats = self.image_formats

        local = self.local

        vision = self.vision

        model: None | str | Unset
        if isinstance(self.model, Unset):
            model = UNSET
        else:
            model = self.model

        reasoning_profile: None | str | Unset
        if isinstance(self.reasoning_profile, Unset):
            reasoning_profile = UNSET
        elif isinstance(self.reasoning_profile, ReasoningProfileDto):
            reasoning_profile = self.reasoning_profile.value
        else:
            reasoning_profile = self.reasoning_profile

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "image_formats": image_formats,
                "local": local,
                "vision": vision,
            }
        )
        if model is not UNSET:
            field_dict["model"] = model
        if reasoning_profile is not UNSET:
            field_dict["reasoning_profile"] = reasoning_profile

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        image_formats = cast(list[str], d.pop("image_formats"))

        local = d.pop("local")

        vision = d.pop("vision")

        def _parse_model(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        model = _parse_model(d.pop("model", UNSET))

        def _parse_reasoning_profile(data: object) -> None | ReasoningProfileDto | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                reasoning_profile_type_1 = ReasoningProfileDto(data)

                return reasoning_profile_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | ReasoningProfileDto | Unset, data)

        reasoning_profile = _parse_reasoning_profile(d.pop("reasoning_profile", UNSET))

        session_model_capabilities = cls(
            image_formats=image_formats,
            local=local,
            vision=vision,
            model=model,
            reasoning_profile=reasoning_profile,
        )

        session_model_capabilities.additional_properties = d
        return session_model_capabilities

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
