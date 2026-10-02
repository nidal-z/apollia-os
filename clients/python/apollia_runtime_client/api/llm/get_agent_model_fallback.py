from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_model_fallback_response import AgentModelFallbackResponse
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    agent: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/llm/agents/{agent}/model/fallback".format(
            agent=quote(str(agent), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentModelFallbackResponse | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AgentModelFallbackResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentModelFallbackResponse | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    agent: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[AgentModelFallbackResponse | ApiErrorBody]:
    """`GET /api/v1/llm/agents/{agent}/model/fallback`: the last time the agent's
    model was not available since the runtime started, for its sheet.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentModelFallbackResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        agent=agent,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    agent: str,
    *,
    client: AuthenticatedClient | Client,
) -> AgentModelFallbackResponse | ApiErrorBody | None:
    """`GET /api/v1/llm/agents/{agent}/model/fallback`: the last time the agent's
    model was not available since the runtime started, for its sheet.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentModelFallbackResponse | ApiErrorBody
    """

    return sync_detailed(
        agent=agent,
        client=client,
    ).parsed


async def asyncio_detailed(
    agent: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[AgentModelFallbackResponse | ApiErrorBody]:
    """`GET /api/v1/llm/agents/{agent}/model/fallback`: the last time the agent's
    model was not available since the runtime started, for its sheet.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentModelFallbackResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        agent=agent,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    agent: str,
    *,
    client: AuthenticatedClient | Client,
) -> AgentModelFallbackResponse | ApiErrorBody | None:
    """`GET /api/v1/llm/agents/{agent}/model/fallback`: the last time the agent's
    model was not available since the runtime started, for its sheet.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentModelFallbackResponse | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            agent=agent,
            client=client,
        )
    ).parsed
