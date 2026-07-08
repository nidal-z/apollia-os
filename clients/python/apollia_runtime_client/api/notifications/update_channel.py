from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.channel_response import ChannelResponse
from ...models.update_channel_request import UpdateChannelRequest
from ...types import Response


def _get_kwargs(
    id: str,
    *,
    body: UpdateChannelRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/notifications/channels/{id}".format(
            id=quote(str(id), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ChannelResponse | None:
    if response.status_code == 200:
        response_200 = ChannelResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 422:
        response_422 = ApiErrorBody.from_dict(response.json())

        return response_422

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
) -> Response[ApiErrorBody | ChannelResponse]:
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
    body: UpdateChannelRequest,
) -> Response[ApiErrorBody | ChannelResponse]:
    """`PUT /api/v1/notifications/channels/:id`, update an existing channel.

     Updates the channel in `notifications.db`, then reloads the
    [`NotificationEngine`].

    Args:
        id (str):
        body (UpdateChannelRequest): Request body for `PUT /api/v1/notifications/channels/:id`.

            The `label` field uses a double `Option`:
            - absent from JSON: `None`, keep the existing label;
            - `null`: `Some(None)`, clear the label;
            - `"text"`: `Some(Some("text"))`, replace it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ChannelResponse]
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
    body: UpdateChannelRequest,
) -> ApiErrorBody | ChannelResponse | None:
    """`PUT /api/v1/notifications/channels/:id`, update an existing channel.

     Updates the channel in `notifications.db`, then reloads the
    [`NotificationEngine`].

    Args:
        id (str):
        body (UpdateChannelRequest): Request body for `PUT /api/v1/notifications/channels/:id`.

            The `label` field uses a double `Option`:
            - absent from JSON: `None`, keep the existing label;
            - `null`: `Some(None)`, clear the label;
            - `"text"`: `Some(Some("text"))`, replace it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ChannelResponse
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
    body: UpdateChannelRequest,
) -> Response[ApiErrorBody | ChannelResponse]:
    """`PUT /api/v1/notifications/channels/:id`, update an existing channel.

     Updates the channel in `notifications.db`, then reloads the
    [`NotificationEngine`].

    Args:
        id (str):
        body (UpdateChannelRequest): Request body for `PUT /api/v1/notifications/channels/:id`.

            The `label` field uses a double `Option`:
            - absent from JSON: `None`, keep the existing label;
            - `null`: `Some(None)`, clear the label;
            - `"text"`: `Some(Some("text"))`, replace it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ChannelResponse]
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
    body: UpdateChannelRequest,
) -> ApiErrorBody | ChannelResponse | None:
    """`PUT /api/v1/notifications/channels/:id`, update an existing channel.

     Updates the channel in `notifications.db`, then reloads the
    [`NotificationEngine`].

    Args:
        id (str):
        body (UpdateChannelRequest): Request body for `PUT /api/v1/notifications/channels/:id`.

            The `label` field uses a double `Option`:
            - absent from JSON: `None`, keep the existing label;
            - `null`: `Some(None)`, clear the label;
            - `"text"`: `Some(Some("text"))`, replace it.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ChannelResponse
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            body=body,
        )
    ).parsed
