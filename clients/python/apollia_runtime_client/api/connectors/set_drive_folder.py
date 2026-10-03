from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.drive_folder_status import DriveFolderStatus
from ...models.set_drive_folder_body import SetDriveFolderBody
from ...types import Response


def _get_kwargs(
    account_id: str,
    *,
    body: SetDriveFolderBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/connectors/drive-folders/{account_id}".format(
            account_id=quote(str(account_id), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | DriveFolderStatus | None:
    if response.status_code == 200:
        response_200 = DriveFolderStatus.from_dict(response.json())

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
) -> Response[ApiErrorBody | DriveFolderStatus]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetDriveFolderBody,
) -> Response[ApiErrorBody | DriveFolderStatus]:
    """`PUT /api/v1/connectors/drive-folders/{account_id}`: record the Drive
    folder of one Google account.

    Args:
        account_id (str):
        body (SetDriveFolderBody): Request body for `PUT /api/v1/connectors/drive-
            folders/{account_id}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | DriveFolderStatus]
    """

    kwargs = _get_kwargs(
        account_id=account_id,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetDriveFolderBody,
) -> ApiErrorBody | DriveFolderStatus | None:
    """`PUT /api/v1/connectors/drive-folders/{account_id}`: record the Drive
    folder of one Google account.

    Args:
        account_id (str):
        body (SetDriveFolderBody): Request body for `PUT /api/v1/connectors/drive-
            folders/{account_id}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | DriveFolderStatus
    """

    return sync_detailed(
        account_id=account_id,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetDriveFolderBody,
) -> Response[ApiErrorBody | DriveFolderStatus]:
    """`PUT /api/v1/connectors/drive-folders/{account_id}`: record the Drive
    folder of one Google account.

    Args:
        account_id (str):
        body (SetDriveFolderBody): Request body for `PUT /api/v1/connectors/drive-
            folders/{account_id}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | DriveFolderStatus]
    """

    kwargs = _get_kwargs(
        account_id=account_id,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    account_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetDriveFolderBody,
) -> ApiErrorBody | DriveFolderStatus | None:
    """`PUT /api/v1/connectors/drive-folders/{account_id}`: record the Drive
    folder of one Google account.

    Args:
        account_id (str):
        body (SetDriveFolderBody): Request body for `PUT /api/v1/connectors/drive-
            folders/{account_id}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | DriveFolderStatus
    """

    return (
        await asyncio_detailed(
            account_id=account_id,
            client=client,
            body=body,
        )
    ).parsed
