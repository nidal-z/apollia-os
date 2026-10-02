from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.workflow_error_response import WorkflowErrorResponse
from ...models.workflow_run_view import WorkflowRunView
from ...types import Response


def _get_kwargs(
    id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/workflow-runs/{id}/cancel".format(
            id=quote(str(id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> WorkflowErrorResponse | WorkflowRunView | None:
    if response.status_code == 200:
        response_200 = WorkflowRunView.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = WorkflowErrorResponse.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = WorkflowErrorResponse.from_dict(response.json())

        return response_409

    if response.status_code == 503:
        response_503 = WorkflowErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[WorkflowErrorResponse | WorkflowRunView]:
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
) -> Response[WorkflowErrorResponse | WorkflowRunView]:
    """`POST /api/v1/workflow-runs/{id}/cancel`: stop a run.

     The step in progress is interrupted; the answer comes once the run has
    ended, `canceled`, its remaining steps `not_started`.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowRunView]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> WorkflowErrorResponse | WorkflowRunView | None:
    """`POST /api/v1/workflow-runs/{id}/cancel`: stop a run.

     The step in progress is interrupted; the answer comes once the run has
    ended, `canceled`, its remaining steps `not_started`.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowRunView
    """

    return sync_detailed(
        id=id,
        client=client,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[WorkflowErrorResponse | WorkflowRunView]:
    """`POST /api/v1/workflow-runs/{id}/cancel`: stop a run.

     The step in progress is interrupted; the answer comes once the run has
    ended, `canceled`, its remaining steps `not_started`.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowRunView]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> WorkflowErrorResponse | WorkflowRunView | None:
    """`POST /api/v1/workflow-runs/{id}/cancel`: stop a run.

     The step in progress is interrupted; the answer comes once the run has
    ended, `canceled`, its remaining steps `not_started`.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowRunView
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
        )
    ).parsed
