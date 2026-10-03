from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.batch_accesses_view import BatchAccessesView
from ...models.batch_error_response import BatchErrorResponse
from ...types import UNSET, Response


def _get_kwargs(
    *,
    agent: str,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["agent"] = agent

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/batches/accesses",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> BatchAccessesView | BatchErrorResponse | None:
    if response.status_code == 200:
        response_200 = BatchAccessesView.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = BatchErrorResponse.from_dict(response.json())

        return response_400

    if response.status_code == 503:
        response_503 = BatchErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[BatchAccessesView | BatchErrorResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    agent: str,
) -> Response[BatchAccessesView | BatchErrorResponse]:
    """`GET /api/v1/batches/accesses`: the sensitive accesses of an agent, those
    a launch may grant for the duration of a batch.

     Reading is never listed: it runs without asking. An access that is not
    `grantable` (executing code, or an access nobody classified) is listed
    too: an item that reaches for it waits for a decision.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchAccessesView | BatchErrorResponse]
    """

    kwargs = _get_kwargs(
        agent=agent,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    agent: str,
) -> BatchAccessesView | BatchErrorResponse | None:
    """`GET /api/v1/batches/accesses`: the sensitive accesses of an agent, those
    a launch may grant for the duration of a batch.

     Reading is never listed: it runs without asking. An access that is not
    `grantable` (executing code, or an access nobody classified) is listed
    too: an item that reaches for it waits for a decision.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchAccessesView | BatchErrorResponse
    """

    return sync_detailed(
        client=client,
        agent=agent,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    agent: str,
) -> Response[BatchAccessesView | BatchErrorResponse]:
    """`GET /api/v1/batches/accesses`: the sensitive accesses of an agent, those
    a launch may grant for the duration of a batch.

     Reading is never listed: it runs without asking. An access that is not
    `grantable` (executing code, or an access nobody classified) is listed
    too: an item that reaches for it waits for a decision.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchAccessesView | BatchErrorResponse]
    """

    kwargs = _get_kwargs(
        agent=agent,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    agent: str,
) -> BatchAccessesView | BatchErrorResponse | None:
    """`GET /api/v1/batches/accesses`: the sensitive accesses of an agent, those
    a launch may grant for the duration of a batch.

     Reading is never listed: it runs without asking. An access that is not
    `grantable` (executing code, or an access nobody classified) is listed
    too: an item that reaches for it waits for a decision.

    Args:
        agent (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchAccessesView | BatchErrorResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            agent=agent,
        )
    ).parsed
