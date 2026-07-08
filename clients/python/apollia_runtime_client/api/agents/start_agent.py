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

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

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

     Loads the Python agent module via [`AgentLoader`], validates
    AIP duck typing, registers the agent with its real manifest, transitions
    to Active (or Degraded if optional tools are missing), and creates an
    [`ExecutionCoordinator`] registered with the [`TaskRouter`].

    Tool resolution is delegated to [`apollia_tools::resolve`] which handles
    the `a2a:` prefix correctly (A2A dependencies live in the ToolProxy
    allowed_tools list, not in the ToolRegistry, and must not trigger
    DEGRADED). A missing `tools_required` entry returns 400 Bad Request.

    Returns 201 Created with the agent_id and state.
    Returns 400 Bad Request if the Python module is invalid.

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

     Loads the Python agent module via [`AgentLoader`], validates
    AIP duck typing, registers the agent with its real manifest, transitions
    to Active (or Degraded if optional tools are missing), and creates an
    [`ExecutionCoordinator`] registered with the [`TaskRouter`].

    Tool resolution is delegated to [`apollia_tools::resolve`] which handles
    the `a2a:` prefix correctly (A2A dependencies live in the ToolProxy
    allowed_tools list, not in the ToolRegistry, and must not trigger
    DEGRADED). A missing `tools_required` entry returns 400 Bad Request.

    Returns 201 Created with the agent_id and state.
    Returns 400 Bad Request if the Python module is invalid.

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

     Loads the Python agent module via [`AgentLoader`], validates
    AIP duck typing, registers the agent with its real manifest, transitions
    to Active (or Degraded if optional tools are missing), and creates an
    [`ExecutionCoordinator`] registered with the [`TaskRouter`].

    Tool resolution is delegated to [`apollia_tools::resolve`] which handles
    the `a2a:` prefix correctly (A2A dependencies live in the ToolProxy
    allowed_tools list, not in the ToolRegistry, and must not trigger
    DEGRADED). A missing `tools_required` entry returns 400 Bad Request.

    Returns 201 Created with the agent_id and state.
    Returns 400 Bad Request if the Python module is invalid.

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

     Loads the Python agent module via [`AgentLoader`], validates
    AIP duck typing, registers the agent with its real manifest, transitions
    to Active (or Degraded if optional tools are missing), and creates an
    [`ExecutionCoordinator`] registered with the [`TaskRouter`].

    Tool resolution is delegated to [`apollia_tools::resolve`] which handles
    the `a2a:` prefix correctly (A2A dependencies live in the ToolProxy
    allowed_tools list, not in the ToolRegistry, and must not trigger
    DEGRADED). A missing `tools_required` entry returns 400 Bad Request.

    Returns 201 Created with the agent_id and state.
    Returns 400 Bad Request if the Python module is invalid.

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
