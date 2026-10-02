from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.imap_account_body import ImapAccountBody
from ...models.imap_account_response import ImapAccountResponse
from ...models.imap_error_body import ImapErrorBody
from ...types import Response


def _get_kwargs(
    *,
    body: ImapAccountBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/connectors/imap/test",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ImapAccountResponse | ImapErrorBody | None:
    if response.status_code == 200:
        response_200 = ImapAccountResponse.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ImapErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 422:
        response_422 = ImapErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 502:
        response_502 = ImapErrorBody.from_dict(response.json())

        return response_502

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ImapAccountResponse | ImapErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ImapAccountBody,
) -> Response[ImapAccountResponse | ImapErrorBody]:
    """`POST /api/v1/connectors/imap/test`: verify a mailbox without storing it.

    Args:
        body (ImapAccountBody): Body of both mailbox routes. Deliberately without `Debug`: it
            carries the
            password.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ImapAccountResponse | ImapErrorBody]
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
    body: ImapAccountBody,
) -> ImapAccountResponse | ImapErrorBody | None:
    """`POST /api/v1/connectors/imap/test`: verify a mailbox without storing it.

    Args:
        body (ImapAccountBody): Body of both mailbox routes. Deliberately without `Debug`: it
            carries the
            password.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ImapAccountResponse | ImapErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ImapAccountBody,
) -> Response[ImapAccountResponse | ImapErrorBody]:
    """`POST /api/v1/connectors/imap/test`: verify a mailbox without storing it.

    Args:
        body (ImapAccountBody): Body of both mailbox routes. Deliberately without `Debug`: it
            carries the
            password.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ImapAccountResponse | ImapErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: ImapAccountBody,
) -> ImapAccountResponse | ImapErrorBody | None:
    """`POST /api/v1/connectors/imap/test`: verify a mailbox without storing it.

    Args:
        body (ImapAccountBody): Body of both mailbox routes. Deliberately without `Debug`: it
            carries the
            password.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ImapAccountResponse | ImapErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
