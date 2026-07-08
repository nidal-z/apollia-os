from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_response import AgentResponse
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/agents/{id}".format(
            id=quote(str(id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentResponse | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AgentResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = ApiErrorBody.from_dict(response.json())

        return response_409

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
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[AgentResponse | ApiErrorBody]:
    """Handler for `DELETE /api/v1/agents/{id}`.

     Performs a full graceful shutdown of the agent:
    1. Transitions to `Stopping` (emits `AgentStopping` event for observers).
    2. Unregisters the `ExecutionCoordinator` from the `TaskRouter` so no new
       tasks can be submitted to this agent.
    3. Transitions to `Stopped` (emits `AgentStopped` event).

    This mirrors the sequence performed by [`ShutdownController::stop_agents`]
    during full system shutdown. The `Stopping` intermediate state is preserved
    so that event-bus subscribers (dashboard, SSE streams) observe the correct
    lifecycle.

    Accepts both a UUID and a human-readable agent name (e.g. `apollia-reviewer`).
    Returns 409 Conflict if the agent is already stopped or stopping.
    Returns 404 if the agent does not exist.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> AgentResponse | ApiErrorBody | None:
    """Handler for `DELETE /api/v1/agents/{id}`.

     Performs a full graceful shutdown of the agent:
    1. Transitions to `Stopping` (emits `AgentStopping` event for observers).
    2. Unregisters the `ExecutionCoordinator` from the `TaskRouter` so no new
       tasks can be submitted to this agent.
    3. Transitions to `Stopped` (emits `AgentStopped` event).

    This mirrors the sequence performed by [`ShutdownController::stop_agents`]
    during full system shutdown. The `Stopping` intermediate state is preserved
    so that event-bus subscribers (dashboard, SSE streams) observe the correct
    lifecycle.

    Accepts both a UUID and a human-readable agent name (e.g. `apollia-reviewer`).
    Returns 409 Conflict if the agent is already stopped or stopping.
    Returns 404 if the agent does not exist.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentResponse | ApiErrorBody
    """

    return sync_detailed(
        id=id,
        client=client,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[AgentResponse | ApiErrorBody]:
    """Handler for `DELETE /api/v1/agents/{id}`.

     Performs a full graceful shutdown of the agent:
    1. Transitions to `Stopping` (emits `AgentStopping` event for observers).
    2. Unregisters the `ExecutionCoordinator` from the `TaskRouter` so no new
       tasks can be submitted to this agent.
    3. Transitions to `Stopped` (emits `AgentStopped` event).

    This mirrors the sequence performed by [`ShutdownController::stop_agents`]
    during full system shutdown. The `Stopping` intermediate state is preserved
    so that event-bus subscribers (dashboard, SSE streams) observe the correct
    lifecycle.

    Accepts both a UUID and a human-readable agent name (e.g. `apollia-reviewer`).
    Returns 409 Conflict if the agent is already stopped or stopping.
    Returns 404 if the agent does not exist.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> AgentResponse | ApiErrorBody | None:
    """Handler for `DELETE /api/v1/agents/{id}`.

     Performs a full graceful shutdown of the agent:
    1. Transitions to `Stopping` (emits `AgentStopping` event for observers).
    2. Unregisters the `ExecutionCoordinator` from the `TaskRouter` so no new
       tasks can be submitted to this agent.
    3. Transitions to `Stopped` (emits `AgentStopped` event).

    This mirrors the sequence performed by [`ShutdownController::stop_agents`]
    during full system shutdown. The `Stopping` intermediate state is preserved
    so that event-bus subscribers (dashboard, SSE streams) observe the correct
    lifecycle.

    Accepts both a UUID and a human-readable agent name (e.g. `apollia-reviewer`).
    Returns 409 Conflict if the agent is already stopped or stopping.
    Returns 404 if the agent does not exist.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentResponse | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
        )
    ).parsed
