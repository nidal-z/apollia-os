from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.parse_automation_request import ParseAutomationRequest
from ...models.parse_automation_response_200 import ParseAutomationResponse200
from ...types import Response


def _get_kwargs(
    *,
    body: ParseAutomationRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/assists/automation",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ParseAutomationResponse200 | None:
    if response.status_code == 200:
        response_200 = ParseAutomationResponse200.from_dict(response.json())

        return response_200

    if response.status_code == 422:
        response_422 = cast(Any, None)
        return response_422

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | ParseAutomationResponse200]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ParseAutomationRequest,
) -> Response[Any | ParseAutomationResponse200]:
    """`POST /api/v1/assists/automation`: parses an automation described in
    words into a schedule, an agent and a payload, with its ambiguities.

    Args:
        body (ParseAutomationRequest): An automation described in words, and the agents the window
            knows.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ParseAutomationResponse200]
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
    body: ParseAutomationRequest,
) -> Any | ParseAutomationResponse200 | None:
    """`POST /api/v1/assists/automation`: parses an automation described in
    words into a schedule, an agent and a payload, with its ambiguities.

    Args:
        body (ParseAutomationRequest): An automation described in words, and the agents the window
            knows.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ParseAutomationResponse200
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ParseAutomationRequest,
) -> Response[Any | ParseAutomationResponse200]:
    """`POST /api/v1/assists/automation`: parses an automation described in
    words into a schedule, an agent and a payload, with its ambiguities.

    Args:
        body (ParseAutomationRequest): An automation described in words, and the agents the window
            knows.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ParseAutomationResponse200]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: ParseAutomationRequest,
) -> Any | ParseAutomationResponse200 | None:
    """`POST /api/v1/assists/automation`: parses an automation described in
    words into a schedule, an agent and a payload, with its ambiguities.

    Args:
        body (ParseAutomationRequest): An automation described in words, and the agents the window
            knows.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ParseAutomationResponse200
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
