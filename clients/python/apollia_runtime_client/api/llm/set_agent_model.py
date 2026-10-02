from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_model_response import AgentModelResponse
from ...models.api_error_body import ApiErrorBody
from ...models.set_agent_model_request import SetAgentModelRequest
from ...types import Response


def _get_kwargs(
    agent: str,
    *,
    body: SetAgentModelRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/llm/agents/{agent}/model".format(
            agent=quote(str(agent), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentModelResponse | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AgentModelResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 422:
        response_422 = ApiErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentModelResponse | ApiErrorBody]:
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
    body: SetAgentModelRequest,
) -> Response[AgentModelResponse | ApiErrorBody]:
    """`PUT /api/v1/llm/agents/{agent}/model`: assign a model, and optionally a
    preset of it, to an agent. Kept apart from the installed package.

    Args:
        agent (str):
        body (SetAgentModelRequest): Body of `PUT /api/v1/llm/agents/{agent}/model`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentModelResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        agent=agent,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    agent: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetAgentModelRequest,
) -> AgentModelResponse | ApiErrorBody | None:
    """`PUT /api/v1/llm/agents/{agent}/model`: assign a model, and optionally a
    preset of it, to an agent. Kept apart from the installed package.

    Args:
        agent (str):
        body (SetAgentModelRequest): Body of `PUT /api/v1/llm/agents/{agent}/model`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentModelResponse | ApiErrorBody
    """

    return sync_detailed(
        agent=agent,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    agent: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetAgentModelRequest,
) -> Response[AgentModelResponse | ApiErrorBody]:
    """`PUT /api/v1/llm/agents/{agent}/model`: assign a model, and optionally a
    preset of it, to an agent. Kept apart from the installed package.

    Args:
        agent (str):
        body (SetAgentModelRequest): Body of `PUT /api/v1/llm/agents/{agent}/model`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentModelResponse | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        agent=agent,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    agent: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetAgentModelRequest,
) -> AgentModelResponse | ApiErrorBody | None:
    """`PUT /api/v1/llm/agents/{agent}/model`: assign a model, and optionally a
    preset of it, to an agent. Kept apart from the installed package.

    Args:
        agent (str):
        body (SetAgentModelRequest): Body of `PUT /api/v1/llm/agents/{agent}/model`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentModelResponse | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            agent=agent,
            client=client,
            body=body,
        )
    ).parsed
