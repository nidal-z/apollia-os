from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_list_response import AgentListResponse
from ...models.api_error_body import ApiErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    supports_a2a: bool | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["supports_a2a"] = supports_a2a

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/agents",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentListResponse | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AgentListResponse.from_dict(response.json())

        return response_200

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentListResponse | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    supports_a2a: bool | Unset = UNSET,
) -> Response[AgentListResponse | ApiErrorBody]:
    """Handler for `GET /api/v1/agents`.

     Lists registered agents. When `?supports_a2a=true` is present, only agents
    that declare `supports_a2a = true` in their manifest are returned, and each
    entry includes the agent's `skills` and `version`.

    Args:
        supports_a2a (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentListResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        supports_a2a=supports_a2a,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    supports_a2a: bool | Unset = UNSET,
) -> AgentListResponse | ApiErrorBody | None:
    """Handler for `GET /api/v1/agents`.

     Lists registered agents. When `?supports_a2a=true` is present, only agents
    that declare `supports_a2a = true` in their manifest are returned, and each
    entry includes the agent's `skills` and `version`.

    Args:
        supports_a2a (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentListResponse | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        supports_a2a=supports_a2a,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    supports_a2a: bool | Unset = UNSET,
) -> Response[AgentListResponse | ApiErrorBody]:
    """Handler for `GET /api/v1/agents`.

     Lists registered agents. When `?supports_a2a=true` is present, only agents
    that declare `supports_a2a = true` in their manifest are returned, and each
    entry includes the agent's `skills` and `version`.

    Args:
        supports_a2a (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentListResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        supports_a2a=supports_a2a,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    supports_a2a: bool | Unset = UNSET,
) -> AgentListResponse | ApiErrorBody | None:
    """Handler for `GET /api/v1/agents`.

     Lists registered agents. When `?supports_a2a=true` is present, only agents
    that declare `supports_a2a = true` in their manifest are returned, and each
    entry includes the agent's `skills` and `version`.

    Args:
        supports_a2a (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentListResponse | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            supports_a2a=supports_a2a,
        )
    ).parsed
