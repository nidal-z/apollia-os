from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.permission_rule_dto import PermissionRuleDto
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    scope: str | Unset = UNSET,
    tool_name: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["scope"] = scope

    params["tool_name"] = tool_name

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/governance/rules",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | list[PermissionRuleDto] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = PermissionRuleDto.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | list[PermissionRuleDto]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    scope: str | Unset = UNSET,
    tool_name: str | Unset = UNSET,
) -> Response[ApiErrorBody | list[PermissionRuleDto]]:
    """`GET /api/v1/governance/rules`: the `agent` and `global` permission rules.

    Args:
        scope (str | Unset):
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[PermissionRuleDto]]
    """

    kwargs = _get_kwargs(
        scope=scope,
        tool_name=tool_name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    scope: str | Unset = UNSET,
    tool_name: str | Unset = UNSET,
) -> ApiErrorBody | list[PermissionRuleDto] | None:
    """`GET /api/v1/governance/rules`: the `agent` and `global` permission rules.

    Args:
        scope (str | Unset):
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[PermissionRuleDto]
    """

    return sync_detailed(
        client=client,
        scope=scope,
        tool_name=tool_name,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    scope: str | Unset = UNSET,
    tool_name: str | Unset = UNSET,
) -> Response[ApiErrorBody | list[PermissionRuleDto]]:
    """`GET /api/v1/governance/rules`: the `agent` and `global` permission rules.

    Args:
        scope (str | Unset):
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[PermissionRuleDto]]
    """

    kwargs = _get_kwargs(
        scope=scope,
        tool_name=tool_name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    scope: str | Unset = UNSET,
    tool_name: str | Unset = UNSET,
) -> ApiErrorBody | list[PermissionRuleDto] | None:
    """`GET /api/v1/governance/rules`: the `agent` and `global` permission rules.

    Args:
        scope (str | Unset):
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[PermissionRuleDto]
    """

    return (
        await asyncio_detailed(
            client=client,
            scope=scope,
            tool_name=tool_name,
        )
    ).parsed
