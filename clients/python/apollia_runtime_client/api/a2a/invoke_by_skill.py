from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.a2a_invocation_result import A2AInvocationResult
from ...models.api_error_body import ApiErrorBody
from ...models.invoke_request import InvokeRequest
from ...types import Response


def _get_kwargs(
    *,
    body: InvokeRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/a2a/invoke",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> A2AInvocationResult | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = A2AInvocationResult.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if response.status_code == 504:
        response_504 = ApiErrorBody.from_dict(response.json())

        return response_504

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[A2AInvocationResult | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: InvokeRequest,
) -> Response[A2AInvocationResult | ApiErrorBody]:
    """Handler for `POST /api/v1/a2a/invoke`.

     Invokes a Worker Agent by skill ID via the [`A2AInvoker`].
    Returns 503 if the invoker is not initialized.
    Returns 404 if the skill is not found, 503 if the agent is not active.

    Args:
        body (InvokeRequest): Request body for `POST /api/v1/a2a/invoke`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[A2AInvocationResult | ApiErrorBody]
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
    body: InvokeRequest,
) -> A2AInvocationResult | ApiErrorBody | None:
    """Handler for `POST /api/v1/a2a/invoke`.

     Invokes a Worker Agent by skill ID via the [`A2AInvoker`].
    Returns 503 if the invoker is not initialized.
    Returns 404 if the skill is not found, 503 if the agent is not active.

    Args:
        body (InvokeRequest): Request body for `POST /api/v1/a2a/invoke`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        A2AInvocationResult | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: InvokeRequest,
) -> Response[A2AInvocationResult | ApiErrorBody]:
    """Handler for `POST /api/v1/a2a/invoke`.

     Invokes a Worker Agent by skill ID via the [`A2AInvoker`].
    Returns 503 if the invoker is not initialized.
    Returns 404 if the skill is not found, 503 if the agent is not active.

    Args:
        body (InvokeRequest): Request body for `POST /api/v1/a2a/invoke`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[A2AInvocationResult | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: InvokeRequest,
) -> A2AInvocationResult | ApiErrorBody | None:
    """Handler for `POST /api/v1/a2a/invoke`.

     Invokes a Worker Agent by skill ID via the [`A2AInvoker`].
    Returns 503 if the invoker is not initialized.
    Returns 404 if the skill is not found, 503 if the agent is not active.

    Args:
        body (InvokeRequest): Request body for `POST /api/v1/a2a/invoke`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        A2AInvocationResult | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
