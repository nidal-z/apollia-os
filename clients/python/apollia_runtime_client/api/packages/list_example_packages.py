from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.example_package import ExamplePackage
from ...models.package_error_body import PackageErrorBody
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/packages/examples",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> PackageErrorBody | list[ExamplePackage] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = ExamplePackage.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if response.status_code == 500:
        response_500 = PackageErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[PackageErrorBody | list[ExamplePackage]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[PackageErrorBody | list[ExamplePackage]]:
    """`GET /api/v1/packages/examples`: the example packages the binary carries,
    each with its agents and their mission, the connections it requires
    (present or missing on this machine) and the installed version when the
    user installed it. None of them is installed until the user chooses it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PackageErrorBody | list[ExamplePackage]]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> PackageErrorBody | list[ExamplePackage] | None:
    """`GET /api/v1/packages/examples`: the example packages the binary carries,
    each with its agents and their mission, the connections it requires
    (present or missing on this machine) and the installed version when the
    user installed it. None of them is installed until the user chooses it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PackageErrorBody | list[ExamplePackage]
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[PackageErrorBody | list[ExamplePackage]]:
    """`GET /api/v1/packages/examples`: the example packages the binary carries,
    each with its agents and their mission, the connections it requires
    (present or missing on this machine) and the installed version when the
    user installed it. None of them is installed until the user chooses it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PackageErrorBody | list[ExamplePackage]]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> PackageErrorBody | list[ExamplePackage] | None:
    """`GET /api/v1/packages/examples`: the example packages the binary carries,
    each with its agents and their mission, the connections it requires
    (present or missing on this machine) and the installed version when the
    user installed it. None of them is installed until the user chooses it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PackageErrorBody | list[ExamplePackage]
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
