from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.reload_router_response_backends_item import ReloadRouterResponseBackendsItem





T = TypeVar("T", bound="ReloadRouterResponse")



@_attrs_define
class ReloadRouterResponse:
    """ Response body for `POST /api/v1/llm/reload`.

    Carries the list of backends that are live in the freshly-swapped router,
    so callers can confirm at a glance what is now available without a
    follow-up `GET /api/v1/llm/status` call.

        Attributes:
            backends (list[ReloadRouterResponseBackendsItem]): Backends now reachable via the active router.
            default (str): Default backend name reported by the router (empty when no backends).
            reaches_running_agents (bool): Whether the rebuilt router reaches agents that are already running.

                `false` today, and not a transient state: the agent execution path reads
                its router from a `OnceLock` populated at boot, a different cell from the
                one this route rewrites. A reload therefore reaches chat and this API,
                and an already-running agent keeps the router it started with until the
                daemon restarts. Reported rather than hidden, because the failure it
                produces on the Python side, `'NoneType' object has no attribute
                'complete'`, names none of this.
     """

    backends: list[ReloadRouterResponseBackendsItem]
    default: str
    reaches_running_agents: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.reload_router_response_backends_item import ReloadRouterResponseBackendsItem
        backends = []
        for backends_item_data in self.backends:
            backends_item = backends_item_data.to_dict()
            backends.append(backends_item)



        default = self.default

        reaches_running_agents = self.reaches_running_agents


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backends": backends,
            "default": default,
            "reaches_running_agents": reaches_running_agents,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.reload_router_response_backends_item import ReloadRouterResponseBackendsItem
        d = dict(src_dict)
        backends = []
        _backends = d.pop("backends")
        for backends_item_data in (_backends):
            backends_item = ReloadRouterResponseBackendsItem.from_dict(backends_item_data)



            backends.append(backends_item)


        default = d.pop("default")

        reaches_running_agents = d.pop("reaches_running_agents")

        reload_router_response = cls(
            backends=backends,
            default=default,
            reaches_running_agents=reaches_running_agents,
        )


        reload_router_response.additional_properties = d
        return reload_router_response

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
