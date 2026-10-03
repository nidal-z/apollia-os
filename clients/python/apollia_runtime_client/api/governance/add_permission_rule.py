from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.add_permission_rule_body import AddPermissionRuleBody
from ...models.added_permission_rule import AddedPermissionRule
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    *,
    body: AddPermissionRuleBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/permissions/rules",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AddedPermissionRule | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = AddedPermissionRule.from_dict(response.json())

        return response_200

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
) -> Response[AddedPermissionRule | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: AddPermissionRuleBody,
) -> Response[AddedPermissionRule | ApiErrorBody]:
    """`POST /api/v1/permissions/rules`: adds a prefix rule for every agent.

    Args:
        body (AddPermissionRuleBody): Request body for `POST /api/v1/permissions/rules`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AddedPermissionRule | ApiErrorBody]
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
    body: AddPermissionRuleBody,
) -> AddedPermissionRule | ApiErrorBody | None:
    """`POST /api/v1/permissions/rules`: adds a prefix rule for every agent.

    Args:
        body (AddPermissionRuleBody): Request body for `POST /api/v1/permissions/rules`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AddedPermissionRule | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: AddPermissionRuleBody,
) -> Response[AddedPermissionRule | ApiErrorBody]:
    """`POST /api/v1/permissions/rules`: adds a prefix rule for every agent.

    Args:
        body (AddPermissionRuleBody): Request body for `POST /api/v1/permissions/rules`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AddedPermissionRule | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: AddPermissionRuleBody,
) -> AddedPermissionRule | ApiErrorBody | None:
    """`POST /api/v1/permissions/rules`: adds a prefix rule for every agent.

    Args:
        body (AddPermissionRuleBody): Request body for `POST /api/v1/permissions/rules`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AddedPermissionRule | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
