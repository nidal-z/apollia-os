from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.package_error_body import PackageErrorBody
from ...types import Response


def _get_kwargs(
    fingerprint: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/packages/trusted-keys/{fingerprint}".format(
            fingerprint=quote(str(fingerprint), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | PackageErrorBody | None:
    if response.status_code == 200:
        response_200 = cast(Any, None)
        return response_200

    if response.status_code == 403:
        response_403 = PackageErrorBody.from_dict(response.json())

        return response_403

    if response.status_code == 404:
        response_404 = PackageErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 500:
        response_500 = PackageErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | PackageErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    fingerprint: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | PackageErrorBody]:
    """`DELETE /api/v1/packages/trusted-keys/{fingerprint}`: stop trusting a
    publisher key.

    Args:
        fingerprint (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | PackageErrorBody]
    """

    kwargs = _get_kwargs(
        fingerprint=fingerprint,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    fingerprint: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | PackageErrorBody | None:
    """`DELETE /api/v1/packages/trusted-keys/{fingerprint}`: stop trusting a
    publisher key.

    Args:
        fingerprint (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | PackageErrorBody
    """

    return sync_detailed(
        fingerprint=fingerprint,
        client=client,
    ).parsed


async def asyncio_detailed(
    fingerprint: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | PackageErrorBody]:
    """`DELETE /api/v1/packages/trusted-keys/{fingerprint}`: stop trusting a
    publisher key.

    Args:
        fingerprint (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | PackageErrorBody]
    """

    kwargs = _get_kwargs(
        fingerprint=fingerprint,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    fingerprint: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | PackageErrorBody | None:
    """`DELETE /api/v1/packages/trusted-keys/{fingerprint}`: stop trusting a
    publisher key.

    Args:
        fingerprint (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | PackageErrorBody
    """

    return (
        await asyncio_detailed(
            fingerprint=fingerprint,
            client=client,
        )
    ).parsed
