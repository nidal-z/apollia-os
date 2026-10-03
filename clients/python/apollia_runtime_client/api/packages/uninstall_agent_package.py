from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_package_uninstalled import AgentPackageUninstalled
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    name: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/agent-packages/installed/{name}".format(
            name=quote(str(name), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentPackageUninstalled | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AgentPackageUninstalled.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 403:
        response_403 = ApiErrorBody.from_dict(response.json())

        return response_403

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentPackageUninstalled | ApiErrorBody]:
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
) -> Response[AgentPackageUninstalled | ApiErrorBody]:
    """`DELETE /api/v1/agent-packages/installed/{name}`: uninstall a package
    on the path the command line and `POST /api/v1/packages/{name}/uninstall`
    take: its agents with their overrides, model and triggers, its
    environments and its folder, recorded in the audit journal. The memory
    and the history of its agents are kept.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentPackageUninstalled | ApiErrorBody]
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
) -> AgentPackageUninstalled | ApiErrorBody | None:
    """`DELETE /api/v1/agent-packages/installed/{name}`: uninstall a package
    on the path the command line and `POST /api/v1/packages/{name}/uninstall`
    take: its agents with their overrides, model and triggers, its
    environments and its folder, recorded in the audit journal. The memory
    and the history of its agents are kept.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentPackageUninstalled | ApiErrorBody
    """

    return sync_detailed(
        name=name,
        client=client,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[AgentPackageUninstalled | ApiErrorBody]:
    """`DELETE /api/v1/agent-packages/installed/{name}`: uninstall a package
    on the path the command line and `POST /api/v1/packages/{name}/uninstall`
    take: its agents with their overrides, model and triggers, its
    environments and its folder, recorded in the audit journal. The memory
    and the history of its agents are kept.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentPackageUninstalled | ApiErrorBody]
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
) -> AgentPackageUninstalled | ApiErrorBody | None:
    """`DELETE /api/v1/agent-packages/installed/{name}`: uninstall a package
    on the path the command line and `POST /api/v1/packages/{name}/uninstall`
    take: its agents with their overrides, model and triggers, its
    environments and its folder, recorded in the audit journal. The memory
    and the history of its agents are kept.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentPackageUninstalled | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
        )
    ).parsed
