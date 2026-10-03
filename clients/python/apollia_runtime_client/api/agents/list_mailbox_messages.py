from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.mailbox_message_row import MailboxMessageRow
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    limit: int | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["limit"] = limit

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/mailbox/messages",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | list[MailboxMessageRow] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = MailboxMessageRow.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | list[MailboxMessageRow]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> Response[ApiErrorBody | list[MailboxMessageRow]]:
    """`GET /api/v1/mailbox/messages`: the most recent inter-agent messages.

    Args:
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[MailboxMessageRow]]
    """

    kwargs = _get_kwargs(
        limit=limit,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> ApiErrorBody | list[MailboxMessageRow] | None:
    """`GET /api/v1/mailbox/messages`: the most recent inter-agent messages.

    Args:
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[MailboxMessageRow]
    """

    return sync_detailed(
        client=client,
        limit=limit,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> Response[ApiErrorBody | list[MailboxMessageRow]]:
    """`GET /api/v1/mailbox/messages`: the most recent inter-agent messages.

    Args:
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[MailboxMessageRow]]
    """

    kwargs = _get_kwargs(
        limit=limit,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> ApiErrorBody | list[MailboxMessageRow] | None:
    """`GET /api/v1/mailbox/messages`: the most recent inter-agent messages.

    Args:
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[MailboxMessageRow]
    """

    return (
        await asyncio_detailed(
            client=client,
            limit=limit,
        )
    ).parsed
