from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.plan_decision_request_revised_steps_type_0_item import (
        PlanDecisionRequestRevisedStepsType0Item,
    )


T = TypeVar("T", bound="PlanDecisionRequest")


@_attrs_define
class PlanDecisionRequest:
    """Request body for `POST /api/v1/tasks/{id}/plan-decision`.

    The operator approves the generated plan, rejects it with optional
    feedback used to guide replanning, or submits an edited plan.

        Attributes:
            decision (str): `"approved"` to execute the plan, `"rejected"` to replan, `"edited"` to
                execute `revised_steps` instead.
            feedback (None | str | Unset): Optional feedback injected into the next planning attempt on rejection.
            revised_steps (list[PlanDecisionRequestRevisedStepsType0Item] | None | Unset): The edited plan, required with
                `"edited"`, validated by the engine
                before it runs.
    """

    decision: str
    feedback: None | str | Unset = UNSET
    revised_steps: list[PlanDecisionRequestRevisedStepsType0Item] | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        decision = self.decision

        feedback: None | str | Unset
        if isinstance(self.feedback, Unset):
            feedback = UNSET
        else:
            feedback = self.feedback

        revised_steps: list[dict[str, Any]] | None | Unset
        if isinstance(self.revised_steps, Unset):
            revised_steps = UNSET
        elif isinstance(self.revised_steps, list):
            revised_steps = []
            for revised_steps_type_0_item_data in self.revised_steps:
                revised_steps_type_0_item = revised_steps_type_0_item_data.to_dict()
                revised_steps.append(revised_steps_type_0_item)

        else:
            revised_steps = self.revised_steps

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "decision": decision,
            }
        )
        if feedback is not UNSET:
            field_dict["feedback"] = feedback
        if revised_steps is not UNSET:
            field_dict["revised_steps"] = revised_steps

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.plan_decision_request_revised_steps_type_0_item import (
            PlanDecisionRequestRevisedStepsType0Item,
        )

        d = dict(src_dict)
        decision = d.pop("decision")

        def _parse_feedback(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        feedback = _parse_feedback(d.pop("feedback", UNSET))

        def _parse_revised_steps(
            data: object,
        ) -> list[PlanDecisionRequestRevisedStepsType0Item] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                revised_steps_type_0 = []
                _revised_steps_type_0 = data
                for revised_steps_type_0_item_data in _revised_steps_type_0:
                    revised_steps_type_0_item = PlanDecisionRequestRevisedStepsType0Item.from_dict(
                        revised_steps_type_0_item_data
                    )

                    revised_steps_type_0.append(revised_steps_type_0_item)

                return revised_steps_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[PlanDecisionRequestRevisedStepsType0Item] | None | Unset, data)

        revised_steps = _parse_revised_steps(d.pop("revised_steps", UNSET))

        plan_decision_request = cls(
            decision=decision,
            feedback=feedback,
            revised_steps=revised_steps,
        )

        plan_decision_request.additional_properties = d
        return plan_decision_request

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
