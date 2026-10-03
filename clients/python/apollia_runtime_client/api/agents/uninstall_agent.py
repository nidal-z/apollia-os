from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_uninstalled import AgentUninstalled
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    name: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/agents/installed/{name}".format(
            name=quote(str(name), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentUninstalled | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AgentUninstalled.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentUninstalled | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[AgentUninstalled | ApiErrorBody]:
    """Handler for `DELETE /api/v1/agents/installed/{name}`.

     Removes the row and the install directory, then the agent from the
    registry, its coordinator dropped for good: a stopped agent would otherwise
    be loaded again by the next task that names it. Emits
    `RuntimeEvent::AgentUninstalled`.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentUninstalled | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> AgentUninstalled | ApiErrorBody | None:
    """Handler for `DELETE /api/v1/agents/installed/{name}`.

     Removes the row and the install directory, then the agent from the
    registry, its coordinator dropped for good: a stopped agent would otherwise
    be loaded again by the next task that names it. Emits
    `RuntimeEvent::AgentUninstalled`.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentUninstalled | ApiErrorBody
    """

    return sync_detailed(
        name=name,
        client=client,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[AgentUninstalled | ApiErrorBody]:
    """Handler for `DELETE /api/v1/agents/installed/{name}`.

     Removes the row and the install directory, then the agent from the
    registry, its coordinator dropped for good: a stopped agent would otherwise
    be loaded again by the next task that names it. Emits
    `RuntimeEvent::AgentUninstalled`.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentUninstalled | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> AgentUninstalled | ApiErrorBody | None:
    """Handler for `DELETE /api/v1/agents/installed/{name}`.

     Removes the row and the install directory, then the agent from the
    registry, its coordinator dropped for good: a stopped agent would otherwise
    be loaded again by the next task that names it. Emits
    `RuntimeEvent::AgentUninstalled`.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentUninstalled | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
        )
    ).parsed
