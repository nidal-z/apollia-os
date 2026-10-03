from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.trigger_onboarding_body import TriggerOnboardingBody
from ...models.trigger_result import TriggerResult
from ...types import Response


def _get_kwargs(
    *,
    body: TriggerOnboardingBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/onboarding/sessions",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | TriggerResult | None:
    if response.status_code == 200:
        response_200 = TriggerResult.from_dict(response.json())

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
) -> Response[ApiErrorBody | TriggerResult]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: TriggerOnboardingBody,
) -> Response[ApiErrorBody | TriggerResult]:
    """`POST /api/v1/onboarding/sessions`: opens a conversation with the
    onboarding agent, for the whole onboarding or one topic.

    Args:
        body (TriggerOnboardingBody): Request body for `POST /api/v1/onboarding/sessions`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TriggerResult]
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
    body: TriggerOnboardingBody,
) -> ApiErrorBody | TriggerResult | None:
    """`POST /api/v1/onboarding/sessions`: opens a conversation with the
    onboarding agent, for the whole onboarding or one topic.

    Args:
        body (TriggerOnboardingBody): Request body for `POST /api/v1/onboarding/sessions`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TriggerResult
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: TriggerOnboardingBody,
) -> Response[ApiErrorBody | TriggerResult]:
    """`POST /api/v1/onboarding/sessions`: opens a conversation with the
    onboarding agent, for the whole onboarding or one topic.

    Args:
        body (TriggerOnboardingBody): Request body for `POST /api/v1/onboarding/sessions`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TriggerResult]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: TriggerOnboardingBody,
) -> ApiErrorBody | TriggerResult | None:
    """`POST /api/v1/onboarding/sessions`: opens a conversation with the
    onboarding agent, for the whole onboarding or one topic.

    Args:
        body (TriggerOnboardingBody): Request body for `POST /api/v1/onboarding/sessions`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TriggerResult
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
