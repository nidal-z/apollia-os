from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    provider: str,
    account_id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/connectors/accounts/{provider}/{account_id}".format(
            provider=quote(str(provider), safe=""),
            account_id=quote(str(account_id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 204:
        response_204 = cast(Any, None)
        return response_204

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
) -> Response[Any | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    provider: str,
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | ApiErrorBody]:
    """`DELETE /api/v1/connectors/accounts/:provider/:account_id`: forget the
    account's token. The provider is not notified.

    Args:
        provider (str):
        account_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        provider=provider,
        account_id=account_id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    provider: str,
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | ApiErrorBody | None:
    """`DELETE /api/v1/connectors/accounts/:provider/:account_id`: forget the
    account's token. The provider is not notified.

    Args:
        provider (str):
        account_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        provider=provider,
        account_id=account_id,
        client=client,
    ).parsed


async def asyncio_detailed(
    provider: str,
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | ApiErrorBody]:
    """`DELETE /api/v1/connectors/accounts/:provider/:account_id`: forget the
    account's token. The provider is not notified.

    Args:
        provider (str):
        account_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        provider=provider,
        account_id=account_id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    provider: str,
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | ApiErrorBody | None:
    """`DELETE /api/v1/connectors/accounts/:provider/:account_id`: forget the
    account's token. The provider is not notified.

    Args:
        provider (str):
        account_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            provider=provider,
            account_id=account_id,
            client=client,
        )
    ).parsed
