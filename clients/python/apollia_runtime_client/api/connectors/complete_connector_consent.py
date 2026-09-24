from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.complete_consent_body import CompleteConsentBody
from ...models.connected_account_response import ConnectedAccountResponse
from ...types import Response


def _get_kwargs(
    *,
    body: CompleteConsentBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/connectors/oauth/complete",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ConnectedAccountResponse | None:
    if response.status_code == 200:
        response_200 = ConnectedAccountResponse.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 502:
        response_502 = ApiErrorBody.from_dict(response.json())

        return response_502

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ConnectedAccountResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CompleteConsentBody,
) -> Response[ApiErrorBody | ConnectedAccountResponse]:
    """`POST /api/v1/connectors/oauth/complete`: exchange the code the provider
    returned and store the account's token.

    Args:
        body (CompleteConsentBody): Body of `POST /api/v1/connectors/oauth/complete`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ConnectedAccountResponse]
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
    body: CompleteConsentBody,
) -> ApiErrorBody | ConnectedAccountResponse | None:
    """`POST /api/v1/connectors/oauth/complete`: exchange the code the provider
    returned and store the account's token.

    Args:
        body (CompleteConsentBody): Body of `POST /api/v1/connectors/oauth/complete`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ConnectedAccountResponse
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CompleteConsentBody,
) -> Response[ApiErrorBody | ConnectedAccountResponse]:
    """`POST /api/v1/connectors/oauth/complete`: exchange the code the provider
    returned and store the account's token.

    Args:
        body (CompleteConsentBody): Body of `POST /api/v1/connectors/oauth/complete`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ConnectedAccountResponse]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: CompleteConsentBody,
) -> ApiErrorBody | ConnectedAccountResponse | None:
    """`POST /api/v1/connectors/oauth/complete`: exchange the code the provider
    returned and store the account's token.

    Args:
        body (CompleteConsentBody): Body of `POST /api/v1/connectors/oauth/complete`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ConnectedAccountResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
