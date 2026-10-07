from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_response import AgentResponse
from ...models.api_error_body import ApiErrorBody
from ...models.start_agent_request import StartAgentRequest
from ...types import Response


def _get_kwargs(
    *,
    body: StartAgentRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/agents",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentResponse | ApiErrorBody | None:
    if response.status_code == 201:
        response_201 = AgentResponse.from_dict(response.json())

        return response_201

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = ApiErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentResponse | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: StartAgentRequest,
) -> Response[AgentResponse | ApiErrorBody]:
    """Handler for `POST /api/v1/agents`.

     Makes a stored agent available: it is registered from the definition
    `agents.db` holds, moved to Active, and given an [`ExecutionCoordinator`]
    in the [`TaskRouter`]. An agent is data: nothing is imported, and what it
    may reach is read from its resolved access when a run starts.

    Returns 201 Created with the agent_id and state.
    Returns 404 Not Found when no stored agent carries the identifier.
    Returns 409 Conflict for a disabled agent, an agent already available,
    and the Apollia agent, which is always there.

    [`ExecutionCoordinator`]: crate::coordinator::ExecutionCoordinator
    [`TaskRouter`]: crate::router::TaskRouter

    Args:
        body (StartAgentRequest): Request body for `POST /api/v1/agents`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    body: StartAgentRequest,
) -> AgentResponse | ApiErrorBody | None:
    """Handler for `POST /api/v1/agents`.

     Makes a stored agent available: it is registered from the definition
    `agents.db` holds, moved to Active, and given an [`ExecutionCoordinator`]
    in the [`TaskRouter`]. An agent is data: nothing is imported, and what it
    may reach is read from its resolved access when a run starts.

    Returns 201 Created with the agent_id and state.
    Returns 404 Not Found when no stored agent carries the identifier.
    Returns 409 Conflict for a disabled agent, an agent already available,
    and the Apollia agent, which is always there.

    [`ExecutionCoordinator`]: crate::coordinator::ExecutionCoordinator
    [`TaskRouter`]: crate::router::TaskRouter

    Args:
        body (StartAgentRequest): Request body for `POST /api/v1/agents`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentResponse | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: StartAgentRequest,
) -> Response[AgentResponse | ApiErrorBody]:
    """Handler for `POST /api/v1/agents`.

     Makes a stored agent available: it is registered from the definition
    `agents.db` holds, moved to Active, and given an [`ExecutionCoordinator`]
    in the [`TaskRouter`]. An agent is data: nothing is imported, and what it
    may reach is read from its resolved access when a run starts.

    Returns 201 Created with the agent_id and state.
    Returns 404 Not Found when no stored agent carries the identifier.
    Returns 409 Conflict for a disabled agent, an agent already available,
    and the Apollia agent, which is always there.

    [`ExecutionCoordinator`]: crate::coordinator::ExecutionCoordinator
    [`TaskRouter`]: crate::router::TaskRouter

    Args:
        body (StartAgentRequest): Request body for `POST /api/v1/agents`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: StartAgentRequest,
) -> AgentResponse | ApiErrorBody | None:
    """Handler for `POST /api/v1/agents`.

     Makes a stored agent available: it is registered from the definition
    `agents.db` holds, moved to Active, and given an [`ExecutionCoordinator`]
    in the [`TaskRouter`]. An agent is data: nothing is imported, and what it
    may reach is read from its resolved access when a run starts.

    Returns 201 Created with the agent_id and state.
    Returns 404 Not Found when no stored agent carries the identifier.
    Returns 409 Conflict for a disabled agent, an agent already available,
    and the Apollia agent, which is always there.

    [`ExecutionCoordinator`]: crate::coordinator::ExecutionCoordinator
    [`TaskRouter`]: crate::router::TaskRouter

    Args:
        body (StartAgentRequest): Request body for `POST /api/v1/agents`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentResponse | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
