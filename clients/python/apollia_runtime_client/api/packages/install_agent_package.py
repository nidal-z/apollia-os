from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.agent_package_install_body import AgentPackageInstallBody
from ...models.agent_package_install_report import AgentPackageInstallReport
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    *,
    body: AgentPackageInstallBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/agent-packages/installed",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AgentPackageInstallReport | ApiErrorBody | None:
    if response.status_code == 201:
        response_201 = AgentPackageInstallReport.from_dict(response.json())

        return response_201

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 422:
        response_422 = ApiErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 428:
        response_428 = ApiErrorBody.from_dict(response.json())

        return response_428

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AgentPackageInstallReport | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: AgentPackageInstallBody,
) -> Response[AgentPackageInstallReport | ApiErrorBody]:
    """`POST /api/v1/agent-packages/installed`: install a package folder.

     Python dependencies need `deps_confirmed`; without it the answer is a
    `428` whose message is `DEPS_CONFIRMATION_REQUIRED:<count>:<list>`. The
    agents installed take tasks at once, and the triggers written are loaded.

    Args:
        body (AgentPackageInstallBody): Body of `POST /api/v1/agent-packages/installed`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentPackageInstallReport | ApiErrorBody]
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
    body: AgentPackageInstallBody,
) -> AgentPackageInstallReport | ApiErrorBody | None:
    """`POST /api/v1/agent-packages/installed`: install a package folder.

     Python dependencies need `deps_confirmed`; without it the answer is a
    `428` whose message is `DEPS_CONFIRMATION_REQUIRED:<count>:<list>`. The
    agents installed take tasks at once, and the triggers written are loaded.

    Args:
        body (AgentPackageInstallBody): Body of `POST /api/v1/agent-packages/installed`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentPackageInstallReport | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: AgentPackageInstallBody,
) -> Response[AgentPackageInstallReport | ApiErrorBody]:
    """`POST /api/v1/agent-packages/installed`: install a package folder.

     Python dependencies need `deps_confirmed`; without it the answer is a
    `428` whose message is `DEPS_CONFIRMATION_REQUIRED:<count>:<list>`. The
    agents installed take tasks at once, and the triggers written are loaded.

    Args:
        body (AgentPackageInstallBody): Body of `POST /api/v1/agent-packages/installed`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AgentPackageInstallReport | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: AgentPackageInstallBody,
) -> AgentPackageInstallReport | ApiErrorBody | None:
    """`POST /api/v1/agent-packages/installed`: install a package folder.

     Python dependencies need `deps_confirmed`; without it the answer is a
    `428` whose message is `DEPS_CONFIRMATION_REQUIRED:<count>:<list>`. The
    agents installed take tasks at once, and the triggers written are loaded.

    Args:
        body (AgentPackageInstallBody): Body of `POST /api/v1/agent-packages/installed`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AgentPackageInstallReport | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
