from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.journal_anchor import JournalAnchor
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/audit/anchor",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | JournalAnchor | None:
    if response.status_code == 200:
        response_200 = JournalAnchor.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | JournalAnchor]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | JournalAnchor]:
    """`GET /api/v1/audit/anchor`, the exportable head anchor of the global chain.

     Storing this off-machine is the only defense against truncation of the
    global tail once the signing key can be compromised. Returns 404 when the
    journal has no entries yet, 503 when the journal is not configured.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | JournalAnchor]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | JournalAnchor | None:
    """`GET /api/v1/audit/anchor`, the exportable head anchor of the global chain.

     Storing this off-machine is the only defense against truncation of the
    global tail once the signing key can be compromised. Returns 404 when the
    journal has no entries yet, 503 when the journal is not configured.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | JournalAnchor
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | JournalAnchor]:
    """`GET /api/v1/audit/anchor`, the exportable head anchor of the global chain.

     Storing this off-machine is the only defense against truncation of the
    global tail once the signing key can be compromised. Returns 404 when the
    journal has no entries yet, 503 when the journal is not configured.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | JournalAnchor]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | JournalAnchor | None:
    """`GET /api/v1/audit/anchor`, the exportable head anchor of the global chain.

     Storing this off-machine is the only defense against truncation of the
    global tail once the signing key can be compromised. Returns 404 when the
    journal has no entries yet, 503 when the journal is not configured.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | JournalAnchor
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
