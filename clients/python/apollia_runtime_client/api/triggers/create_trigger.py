from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.create_trigger_request import CreateTriggerRequest
from ...models.trigger_definition_response import TriggerDefinitionResponse
from ...types import Response


def _get_kwargs(
    *,
    body: CreateTriggerRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/triggers",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | TriggerDefinitionResponse | None:
    if response.status_code == 201:
        response_201 = TriggerDefinitionResponse.from_dict(response.json())

        return response_201

    if response.status_code == 409:
        response_409 = ApiErrorBody.from_dict(response.json())

        return response_409

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
) -> Response[ApiErrorBody | TriggerDefinitionResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CreateTriggerRequest,
) -> Response[ApiErrorBody | TriggerDefinitionResponse]:
    """`POST /api/v1/triggers`, create a new trigger definition.

     Validates the definition, inserts it into `triggers.db`, reloads the engine,
    and returns `201` with the full definition (including `created_at`).

    Args:
        body (CreateTriggerRequest): Request body for `POST /api/v1/triggers`, trigger creation.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TriggerDefinitionResponse]
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
    body: CreateTriggerRequest,
) -> ApiErrorBody | TriggerDefinitionResponse | None:
    """`POST /api/v1/triggers`, create a new trigger definition.

     Validates the definition, inserts it into `triggers.db`, reloads the engine,
    and returns `201` with the full definition (including `created_at`).

    Args:
        body (CreateTriggerRequest): Request body for `POST /api/v1/triggers`, trigger creation.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TriggerDefinitionResponse
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CreateTriggerRequest,
) -> Response[ApiErrorBody | TriggerDefinitionResponse]:
    """`POST /api/v1/triggers`, create a new trigger definition.

     Validates the definition, inserts it into `triggers.db`, reloads the engine,
    and returns `201` with the full definition (including `created_at`).

    Args:
        body (CreateTriggerRequest): Request body for `POST /api/v1/triggers`, trigger creation.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TriggerDefinitionResponse]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: CreateTriggerRequest,
) -> ApiErrorBody | TriggerDefinitionResponse | None:
    """`POST /api/v1/triggers`, create a new trigger definition.

     Validates the definition, inserts it into `triggers.db`, reloads the engine,
    and returns `201` with the full definition (including `created_at`).

    Args:
        body (CreateTriggerRequest): Request body for `POST /api/v1/triggers`, trigger creation.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TriggerDefinitionResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
