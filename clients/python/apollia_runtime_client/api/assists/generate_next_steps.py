from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.generate_next_steps_body import GenerateNextStepsBody
from ...models.next_steps_reply import NextStepsReply
from ...types import Response


def _get_kwargs(
    *,
    body: GenerateNextStepsBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/assists/next-steps",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | NextStepsReply | None:
    if response.status_code == 200:
        response_200 = NextStepsReply.from_dict(response.json())

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
) -> Response[Any | NextStepsReply]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: GenerateNextStepsBody,
) -> Response[Any | NextStepsReply]:
    """`POST /api/v1/assists/next-steps`: up to three next steps cards, drawn by
    the daemon's default model or by the heuristic.

    Args:
        body (GenerateNextStepsBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | NextStepsReply]
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
    body: GenerateNextStepsBody,
) -> Any | NextStepsReply | None:
    """`POST /api/v1/assists/next-steps`: up to three next steps cards, drawn by
    the daemon's default model or by the heuristic.

    Args:
        body (GenerateNextStepsBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | NextStepsReply
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: GenerateNextStepsBody,
) -> Response[Any | NextStepsReply]:
    """`POST /api/v1/assists/next-steps`: up to three next steps cards, drawn by
    the daemon's default model or by the heuristic.

    Args:
        body (GenerateNextStepsBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | NextStepsReply]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: GenerateNextStepsBody,
) -> Any | NextStepsReply | None:
    """`POST /api/v1/assists/next-steps`: up to three next steps cards, drawn by
    the daemon's default model or by the heuristic.

    Args:
        body (GenerateNextStepsBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | NextStepsReply
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
