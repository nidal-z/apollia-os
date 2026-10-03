from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.companion_enabled import CompanionEnabled
from ...types import Response


def _get_kwargs(
    *,
    body: CompanionEnabled,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/companion/enabled",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | CompanionEnabled | None:
    if response.status_code == 200:
        response_200 = CompanionEnabled.from_dict(response.json())

        return response_200

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | CompanionEnabled]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CompanionEnabled,
) -> Response[ApiErrorBody | CompanionEnabled]:
    """`PUT /api/v1/companion/enabled`: turns the companion on or off.

    Args:
        body (CompanionEnabled): Body of `PUT /api/v1/companion/enabled`, and answer of both
            verbs.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | CompanionEnabled]
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
    body: CompanionEnabled,
) -> ApiErrorBody | CompanionEnabled | None:
    """`PUT /api/v1/companion/enabled`: turns the companion on or off.

    Args:
        body (CompanionEnabled): Body of `PUT /api/v1/companion/enabled`, and answer of both
            verbs.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | CompanionEnabled
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CompanionEnabled,
) -> Response[ApiErrorBody | CompanionEnabled]:
    """`PUT /api/v1/companion/enabled`: turns the companion on or off.

    Args:
        body (CompanionEnabled): Body of `PUT /api/v1/companion/enabled`, and answer of both
            verbs.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | CompanionEnabled]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: CompanionEnabled,
) -> ApiErrorBody | CompanionEnabled | None:
    """`PUT /api/v1/companion/enabled`: turns the companion on or off.

    Args:
        body (CompanionEnabled): Body of `PUT /api/v1/companion/enabled`, and answer of both
            verbs.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | CompanionEnabled
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
