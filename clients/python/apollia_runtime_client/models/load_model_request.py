from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.load_model_request_advanced import LoadModelRequestAdvanced


T = TypeVar("T", bound="LoadModelRequest")


@_attrs_define
class LoadModelRequest:
    """Body of the estimate and of the load.

    Attributes:
        model_id (str): The model, by the identifier the model index gives it.
        advanced (LoadModelRequestAdvanced | Unset): Advanced load settings, in the keys a preset's load settings use
            (`n_threads`, `flash_attn`, `cache_type_k`...). Never a system prompt.
        confirmed (bool | Unset): Load even when the load guardrail would refuse (load only).
        n_ctx (int | None | Unset): Context window in tokens; absent leaves the preset's, else the default.
        n_gpu_layers (int | None | Unset): Layers placed on the accelerator; absent leaves the preset's, else the
            planned count.
        remember (bool | Unset): Write these settings into the model's default preset (load only).
    """

    model_id: str
    advanced: LoadModelRequestAdvanced | Unset = UNSET
    confirmed: bool | Unset = UNSET
    n_ctx: int | None | Unset = UNSET
    n_gpu_layers: int | None | Unset = UNSET
    remember: bool | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        model_id = self.model_id

        advanced: dict[str, Any] | Unset = UNSET
        if not isinstance(self.advanced, Unset):
            advanced = self.advanced.to_dict()

        confirmed = self.confirmed

        n_ctx: int | None | Unset
        if isinstance(self.n_ctx, Unset):
            n_ctx = UNSET
        else:
            n_ctx = self.n_ctx

        n_gpu_layers: int | None | Unset
        if isinstance(self.n_gpu_layers, Unset):
            n_gpu_layers = UNSET
        else:
            n_gpu_layers = self.n_gpu_layers

        remember = self.remember

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "modelId": model_id,
            }
        )
        if advanced is not UNSET:
            field_dict["advanced"] = advanced
        if confirmed is not UNSET:
            field_dict["confirmed"] = confirmed
        if n_ctx is not UNSET:
            field_dict["nCtx"] = n_ctx
        if n_gpu_layers is not UNSET:
            field_dict["nGpuLayers"] = n_gpu_layers
        if remember is not UNSET:
            field_dict["remember"] = remember

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.load_model_request_advanced import LoadModelRequestAdvanced

        d = dict(src_dict)
        model_id = d.pop("modelId")

        _advanced = d.pop("advanced", UNSET)
        advanced: LoadModelRequestAdvanced | Unset
        if isinstance(_advanced, Unset):
            advanced = UNSET
        else:
            advanced = LoadModelRequestAdvanced.from_dict(_advanced)

        confirmed = d.pop("confirmed", UNSET)

        def _parse_n_ctx(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        n_ctx = _parse_n_ctx(d.pop("nCtx", UNSET))

        def _parse_n_gpu_layers(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        n_gpu_layers = _parse_n_gpu_layers(d.pop("nGpuLayers", UNSET))

        remember = d.pop("remember", UNSET)

        load_model_request = cls(
            model_id=model_id,
            advanced=advanced,
            confirmed=confirmed,
            n_ctx=n_ctx,
            n_gpu_layers=n_gpu_layers,
            remember=remember,
        )

        load_model_request.additional_properties = d
        return load_model_request

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
