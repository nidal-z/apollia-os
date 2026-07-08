from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_messages_response import AgentMessagesResponse
from ...models.api_error_body import ApiErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    name: str,
    *,
    limit: int | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["limit"] = limit

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/agents/{name}/messages".format(
            name=quote(str(name), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentMessagesResponse | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AgentMessagesResponse.from_dict(response.json())

        return response_200

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentMessagesResponse | ApiErrorBody]:
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
    limit: int | Unset = UNSET,
) -> Response[AgentMessagesResponse | ApiErrorBody]:
    """`GET /api/v1/agents/:name/messages`, list messages for an agent.

     Returns messages from the in-memory mailbox, sorted by `sent_at` descending.
    Returns an empty array when the agent has no messages.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentMessagesResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
        limit=limit,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> AgentMessagesResponse | ApiErrorBody | None:
    """`GET /api/v1/agents/:name/messages`, list messages for an agent.

     Returns messages from the in-memory mailbox, sorted by `sent_at` descending.
    Returns an empty array when the agent has no messages.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentMessagesResponse | ApiErrorBody
    """

    return sync_detailed(
        name=name,
        client=client,
        limit=limit,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> Response[AgentMessagesResponse | ApiErrorBody]:
    """`GET /api/v1/agents/:name/messages`, list messages for an agent.

     Returns messages from the in-memory mailbox, sorted by `sent_at` descending.
    Returns an empty array when the agent has no messages.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentMessagesResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
        limit=limit,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> AgentMessagesResponse | ApiErrorBody | None:
    """`GET /api/v1/agents/:name/messages`, list messages for an agent.

     Returns messages from the in-memory mailbox, sorted by `sent_at` descending.
    Returns an empty array when the agent has no messages.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentMessagesResponse | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
            limit=limit,
        )
    ).parsed
