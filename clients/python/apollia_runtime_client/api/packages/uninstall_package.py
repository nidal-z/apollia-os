from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.package_error_body import PackageErrorBody
from ...models.uninstall_report import UninstallReport
from ...models.uninstall_request import UninstallRequest
from ...types import Response


def _get_kwargs(
    name: str,
    *,
    body: UninstallRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/packages/{name}/uninstall".format(
            name=quote(str(name), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> PackageErrorBody | UninstallReport | None:
    if response.status_code == 200:
        response_200 = UninstallReport.from_dict(response.json())

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

    if response.status_code == 503:
        response_503 = PackageErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[PackageErrorBody | UninstallReport]:
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
    body: UninstallRequest,
) -> Response[PackageErrorBody | UninstallReport]:
    """`POST /api/v1/packages/{name}/uninstall`: uninstall a package.

     Always removed: its agents with their overrides, model and triggers, its
    workflows and tools, its environments and files, and the MCP servers it
    brought. The connections it required stay. The memory and the history of
    its agents and workflows are erased only when the body asks.

    Args:
        name (str):
        body (UninstallRequest): Body of both uninstall routes: what the user chose to erase.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PackageErrorBody | UninstallReport]
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
    body: UninstallRequest,
) -> PackageErrorBody | UninstallReport | None:
    """`POST /api/v1/packages/{name}/uninstall`: uninstall a package.

     Always removed: its agents with their overrides, model and triggers, its
    workflows and tools, its environments and files, and the MCP servers it
    brought. The connections it required stay. The memory and the history of
    its agents and workflows are erased only when the body asks.

    Args:
        name (str):
        body (UninstallRequest): Body of both uninstall routes: what the user chose to erase.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PackageErrorBody | UninstallReport
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
    body: UninstallRequest,
) -> Response[PackageErrorBody | UninstallReport]:
    """`POST /api/v1/packages/{name}/uninstall`: uninstall a package.

     Always removed: its agents with their overrides, model and triggers, its
    workflows and tools, its environments and files, and the MCP servers it
    brought. The connections it required stay. The memory and the history of
    its agents and workflows are erased only when the body asks.

    Args:
        name (str):
        body (UninstallRequest): Body of both uninstall routes: what the user chose to erase.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PackageErrorBody | UninstallReport]
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
    body: UninstallRequest,
) -> PackageErrorBody | UninstallReport | None:
    """`POST /api/v1/packages/{name}/uninstall`: uninstall a package.

     Always removed: its agents with their overrides, model and triggers, its
    workflows and tools, its environments and files, and the MCP servers it
    brought. The connections it required stay. The memory and the history of
    its agents and workflows are erased only when the body asks.

    Args:
        name (str):
        body (UninstallRequest): Body of both uninstall routes: what the user chose to erase.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PackageErrorBody | UninstallReport
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
            body=body,
        )
    ).parsed
