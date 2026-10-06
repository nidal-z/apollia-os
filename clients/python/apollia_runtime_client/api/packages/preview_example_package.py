from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.package_error_body import PackageErrorBody
from ...models.package_preview import PackagePreview
from ...types import Response


def _get_kwargs(
    name: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/packages/examples/{name}/preview".format(
            name=quote(str(name), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> PackageErrorBody | PackagePreview | None:
    if response.status_code == 200:
        response_200 = PackagePreview.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = PackageErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = PackageErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[PackageErrorBody | PackagePreview]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[PackageErrorBody | PackagePreview]:
    """`POST /api/v1/packages/examples/{name}/preview`: what the example
    contains, what it asks for and which connections it requires, as the
    preview of a package file says it. Nothing is written.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PackageErrorBody | PackagePreview]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> PackageErrorBody | PackagePreview | None:
    """`POST /api/v1/packages/examples/{name}/preview`: what the example
    contains, what it asks for and which connections it requires, as the
    preview of a package file says it. Nothing is written.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PackageErrorBody | PackagePreview
    """

    return sync_detailed(
        name=name,
        client=client,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[PackageErrorBody | PackagePreview]:
    """`POST /api/v1/packages/examples/{name}/preview`: what the example
    contains, what it asks for and which connections it requires, as the
    preview of a package file says it. Nothing is written.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PackageErrorBody | PackagePreview]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> PackageErrorBody | PackagePreview | None:
    """`POST /api/v1/packages/examples/{name}/preview`: what the example
    contains, what it asks for and which connections it requires, as the
    preview of a package file says it. Nothing is written.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PackageErrorBody | PackagePreview
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
        )
    ).parsed
