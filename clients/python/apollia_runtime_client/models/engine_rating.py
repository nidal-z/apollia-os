from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_variant import EngineVariant
from ..models.engine_verdict import EngineVerdict
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.engine_rating_reason_type_0 import EngineRatingReasonType0
    from ..models.engine_rating_reason_type_1 import EngineRatingReasonType1
    from ..models.engine_rating_reason_type_2 import EngineRatingReasonType2
    from ..models.engine_rating_reason_type_3 import EngineRatingReasonType3
    from ..models.engine_rating_reason_type_4 import EngineRatingReasonType4
    from ..models.engine_rating_reason_type_5 import EngineRatingReasonType5
    from ..models.engine_rating_reason_type_6 import EngineRatingReasonType6
    from ..models.engine_rating_reason_type_7 import EngineRatingReasonType7
    from ..models.engine_rating_reason_type_8 import EngineRatingReasonType8


T = TypeVar("T", bound="EngineRating")


@_attrs_define
class EngineRating:
    """The verdict on one variant.

    Attributes:
        variant (EngineVariant): A build of an engine for one kind of hardware.
        verdict (EngineVerdict): How well a variant suits this machine.
        reason (EngineRatingReasonType0 | EngineRatingReasonType1 | EngineRatingReasonType2 | EngineRatingReasonType3 |
            EngineRatingReasonType4 | EngineRatingReasonType5 | EngineRatingReasonType6 | EngineRatingReasonType7 |
            EngineRatingReasonType8 | None | Unset):
    """

    variant: EngineVariant
    verdict: EngineVerdict
    reason: (
        EngineRatingReasonType0
        | EngineRatingReasonType1
        | EngineRatingReasonType2
        | EngineRatingReasonType3
        | EngineRatingReasonType4
        | EngineRatingReasonType5
        | EngineRatingReasonType6
        | EngineRatingReasonType7
        | EngineRatingReasonType8
        | None
        | Unset
    ) = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.engine_rating_reason_type_0 import EngineRatingReasonType0
        from ..models.engine_rating_reason_type_1 import EngineRatingReasonType1
        from ..models.engine_rating_reason_type_2 import EngineRatingReasonType2
        from ..models.engine_rating_reason_type_3 import EngineRatingReasonType3
        from ..models.engine_rating_reason_type_4 import EngineRatingReasonType4
        from ..models.engine_rating_reason_type_5 import EngineRatingReasonType5
        from ..models.engine_rating_reason_type_6 import EngineRatingReasonType6
        from ..models.engine_rating_reason_type_7 import EngineRatingReasonType7
        from ..models.engine_rating_reason_type_8 import EngineRatingReasonType8

        variant = self.variant.value

        verdict = self.verdict.value

        reason: dict[str, Any] | None | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        elif isinstance(self.reason, EngineRatingReasonType0):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType1):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType2):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType3):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType4):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType5):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType6):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType7):
            reason = self.reason.to_dict()
        elif isinstance(self.reason, EngineRatingReasonType8):
            reason = self.reason.to_dict()
        else:
            reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "variant": variant,
                "verdict": verdict,
            }
        )
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.engine_rating_reason_type_0 import EngineRatingReasonType0
        from ..models.engine_rating_reason_type_1 import EngineRatingReasonType1
        from ..models.engine_rating_reason_type_2 import EngineRatingReasonType2
        from ..models.engine_rating_reason_type_3 import EngineRatingReasonType3
        from ..models.engine_rating_reason_type_4 import EngineRatingReasonType4
        from ..models.engine_rating_reason_type_5 import EngineRatingReasonType5
        from ..models.engine_rating_reason_type_6 import EngineRatingReasonType6
        from ..models.engine_rating_reason_type_7 import EngineRatingReasonType7
        from ..models.engine_rating_reason_type_8 import EngineRatingReasonType8

        d = dict(src_dict)
        variant = EngineVariant(d.pop("variant"))

        verdict = EngineVerdict(d.pop("verdict"))

        def _parse_reason(
            data: object,
        ) -> (
            EngineRatingReasonType0
            | EngineRatingReasonType1
            | EngineRatingReasonType2
            | EngineRatingReasonType3
            | EngineRatingReasonType4
            | EngineRatingReasonType5
            | EngineRatingReasonType6
            | EngineRatingReasonType7
            | EngineRatingReasonType8
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
                componentsschemas_engine_rating_reason_type_0 = EngineRatingReasonType0.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_1 = EngineRatingReasonType1.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_2 = EngineRatingReasonType2.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_2
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_3 = EngineRatingReasonType3.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_3
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_4 = EngineRatingReasonType4.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_4
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_5 = EngineRatingReasonType5.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_5
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_6 = EngineRatingReasonType6.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_6
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_7 = EngineRatingReasonType7.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_7
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_rating_reason_type_8 = EngineRatingReasonType8.from_dict(
                    data
                )

                return componentsschemas_engine_rating_reason_type_8
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(
                EngineRatingReasonType0
                | EngineRatingReasonType1
                | EngineRatingReasonType2
                | EngineRatingReasonType3
                | EngineRatingReasonType4
                | EngineRatingReasonType5
                | EngineRatingReasonType6
                | EngineRatingReasonType7
                | EngineRatingReasonType8
                | None
                | Unset,
                data,
            )

        reason = _parse_reason(d.pop("reason", UNSET))

        engine_rating = cls(
            variant=variant,
            verdict=verdict,
            reason=reason,
        )

        engine_rating.additional_properties = d
        return engine_rating

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
