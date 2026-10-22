from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.one_drive_choice_view import OneDriveChoiceView
from ...types import Response


def _get_kwargs(
    account_id: str,
    item_id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/connectors/onedrive/{account_id}/choices/{item_id}".format(
            account_id=quote(str(account_id), safe=""),
            item_id=quote(str(item_id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | list[OneDriveChoiceView] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = OneDriveChoiceView.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 409:
        response_409 = ApiErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 502:
        response_502 = ApiErrorBody.from_dict(response.json())

        return response_502

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | list[OneDriveChoiceView]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    account_id: str,
    item_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | list[OneDriveChoiceView]]:
    """`PUT /api/v1/connectors/onedrive/{account_id}/choices/{item_id}`: let
    Apollia read the folder or file `item_id`, as OneDrive describes it.

    Args:
        account_id (str):
        item_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[OneDriveChoiceView]]
    """

    kwargs = _get_kwargs(
        account_id=account_id,
        item_id=item_id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    account_id: str,
    item_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | list[OneDriveChoiceView] | None:
    """`PUT /api/v1/connectors/onedrive/{account_id}/choices/{item_id}`: let
    Apollia read the folder or file `item_id`, as OneDrive describes it.

    Args:
        account_id (str):
        item_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[OneDriveChoiceView]
    """

    return sync_detailed(
        account_id=account_id,
        item_id=item_id,
        client=client,
    ).parsed


async def asyncio_detailed(
    account_id: str,
    item_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | list[OneDriveChoiceView]]:
    """`PUT /api/v1/connectors/onedrive/{account_id}/choices/{item_id}`: let
    Apollia read the folder or file `item_id`, as OneDrive describes it.

    Args:
        account_id (str):
        item_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[OneDriveChoiceView]]
    """

    kwargs = _get_kwargs(
        account_id=account_id,
        item_id=item_id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    account_id: str,
    item_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | list[OneDriveChoiceView] | None:
    """`PUT /api/v1/connectors/onedrive/{account_id}/choices/{item_id}`: let
    Apollia read the folder or file `item_id`, as OneDrive describes it.

    Args:
        account_id (str):
        item_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[OneDriveChoiceView]
    """

    return (
        await asyncio_detailed(
            account_id=account_id,
            item_id=item_id,
            client=client,
        )
    ).parsed
