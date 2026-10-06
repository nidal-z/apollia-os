from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.example_install_request import ExampleInstallRequest
from ...models.install_report import InstallReport
from ...models.package_error_body import PackageErrorBody
from ...types import Response


def _get_kwargs(
    name: str,
    *,
    body: ExampleInstallRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/packages/examples/{name}/install".format(
            name=quote(str(name), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> InstallReport | PackageErrorBody | None:
    if response.status_code == 201:
        response_201 = InstallReport.from_dict(response.json())

        return response_201

    if response.status_code == 404:
        response_404 = PackageErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = PackageErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 428:
        response_428 = PackageErrorBody.from_dict(response.json())

        return response_428

    if response.status_code == 500:
        response_500 = PackageErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = PackageErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[InstallReport | PackageErrorBody]:
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
    body: ExampleInstallRequest,
) -> Response[InstallReport | PackageErrorBody]:
    """`POST /api/v1/packages/examples/{name}/install`: install the example.

     The example is assembled into a package archive and installed like any
    package file: its preview is computed again and its refusal applies, an
    update that adds accesses or connections needs `changesConfirmed`, and the
    installation is recorded in the audit journal. Once installed it is
    uninstalled like any package.

    Args:
        name (str):
        body (ExampleInstallRequest): Body of `POST /api/v1/packages/examples/{name}/install`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[InstallReport | PackageErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    body: ExampleInstallRequest,
) -> InstallReport | PackageErrorBody | None:
    """`POST /api/v1/packages/examples/{name}/install`: install the example.

     The example is assembled into a package archive and installed like any
    package file: its preview is computed again and its refusal applies, an
    update that adds accesses or connections needs `changesConfirmed`, and the
    installation is recorded in the audit journal. Once installed it is
    uninstalled like any package.

    Args:
        name (str):
        body (ExampleInstallRequest): Body of `POST /api/v1/packages/examples/{name}/install`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        InstallReport | PackageErrorBody
    """

    return sync_detailed(
        name=name,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    body: ExampleInstallRequest,
) -> Response[InstallReport | PackageErrorBody]:
    """`POST /api/v1/packages/examples/{name}/install`: install the example.

     The example is assembled into a package archive and installed like any
    package file: its preview is computed again and its refusal applies, an
    update that adds accesses or connections needs `changesConfirmed`, and the
    installation is recorded in the audit journal. Once installed it is
    uninstalled like any package.

    Args:
        name (str):
        body (ExampleInstallRequest): Body of `POST /api/v1/packages/examples/{name}/install`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[InstallReport | PackageErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    body: ExampleInstallRequest,
) -> InstallReport | PackageErrorBody | None:
    """`POST /api/v1/packages/examples/{name}/install`: install the example.

     The example is assembled into a package archive and installed like any
    package file: its preview is computed again and its refusal applies, an
    update that adds accesses or connections needs `changesConfirmed`, and the
    installation is recorded in the audit journal. Once installed it is
    uninstalled like any package.

    Args:
        name (str):
        body (ExampleInstallRequest): Body of `POST /api/v1/packages/examples/{name}/install`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        InstallReport | PackageErrorBody
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
            body=body,
        )
    ).parsed
