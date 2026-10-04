from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.profile_detail_view import ProfileDetailView
from ...models.profile_update import ProfileUpdate
from ...types import Response


def _get_kwargs(
    id: str,
    *,
    body: ProfileUpdate,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "patch",
        "url": "/api/v1/profiles/{id}".format(
            id=quote(str(id), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ProfileDetailView | None:
    if response.status_code == 200:
        response_200 = ProfileDetailView.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ProfileDetailView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ProfileUpdate,
) -> Response[ApiErrorBody | ProfileDetailView]:
    """`PATCH /api/v1/profiles/{id}`: renames a profile and writes its fields.

    Args:
        id (str):
        body (ProfileUpdate): A change to a profile: a new name, new field values, or both. An
            empty
            value empties its field.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ProfileDetailView]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ProfileUpdate,
) -> ApiErrorBody | ProfileDetailView | None:
    """`PATCH /api/v1/profiles/{id}`: renames a profile and writes its fields.

    Args:
        id (str):
        body (ProfileUpdate): A change to a profile: a new name, new field values, or both. An
            empty
            value empties its field.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ProfileDetailView
    """

    return sync_detailed(
        id=id,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ProfileUpdate,
) -> Response[ApiErrorBody | ProfileDetailView]:
    """`PATCH /api/v1/profiles/{id}`: renames a profile and writes its fields.

    Args:
        id (str):
        body (ProfileUpdate): A change to a profile: a new name, new field values, or both. An
            empty
            value empties its field.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ProfileDetailView]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ProfileUpdate,
) -> ApiErrorBody | ProfileDetailView | None:
    """`PATCH /api/v1/profiles/{id}`: renames a profile and writes its fields.

    Args:
        id (str):
        body (ProfileUpdate): A change to a profile: a new name, new field values, or both. An
            empty
            value empties its field.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ProfileDetailView
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            body=body,
        )
    ).parsed
