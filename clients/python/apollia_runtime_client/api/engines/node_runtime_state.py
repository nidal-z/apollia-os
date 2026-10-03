from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.node_runtime_state_type_0 import NodeRuntimeStateType0
from ...models.node_runtime_state_type_1 import NodeRuntimeStateType1
from ...models.node_runtime_state_type_2 import NodeRuntimeStateType2
from ...models.node_runtime_state_type_3 import NodeRuntimeStateType3
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/engines/nodejs",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> (
    NodeRuntimeStateType0
    | NodeRuntimeStateType1
    | NodeRuntimeStateType2
    | NodeRuntimeStateType3
    | None
):
    if response.status_code == 200:

        def _parse_response_200(
            data: object,
        ) -> (
            NodeRuntimeStateType0
            | NodeRuntimeStateType1
            | NodeRuntimeStateType2
            | NodeRuntimeStateType3
        ):
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_node_runtime_state_type_0 = NodeRuntimeStateType0.from_dict(data)

                return componentsschemas_node_runtime_state_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_node_runtime_state_type_1 = NodeRuntimeStateType1.from_dict(data)

                return componentsschemas_node_runtime_state_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_node_runtime_state_type_2 = NodeRuntimeStateType2.from_dict(data)

                return componentsschemas_node_runtime_state_type_2
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            if not isinstance(data, dict):
                raise TypeError()
            componentsschemas_node_runtime_state_type_3 = NodeRuntimeStateType3.from_dict(data)

            return componentsschemas_node_runtime_state_type_3

        response_200 = _parse_response_200(response.json())

        return response_200

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[
    NodeRuntimeStateType0 | NodeRuntimeStateType1 | NodeRuntimeStateType2 | NodeRuntimeStateType3
]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[
    NodeRuntimeStateType0 | NodeRuntimeStateType1 | NodeRuntimeStateType2 | NodeRuntimeStateType3
]:
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[NodeRuntimeStateType0 | NodeRuntimeStateType1 | NodeRuntimeStateType2 | NodeRuntimeStateType3]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> (
    NodeRuntimeStateType0
    | NodeRuntimeStateType1
    | NodeRuntimeStateType2
    | NodeRuntimeStateType3
    | None
):
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        NodeRuntimeStateType0 | NodeRuntimeStateType1 | NodeRuntimeStateType2 | NodeRuntimeStateType3
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[
    NodeRuntimeStateType0 | NodeRuntimeStateType1 | NodeRuntimeStateType2 | NodeRuntimeStateType3
]:
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[NodeRuntimeStateType0 | NodeRuntimeStateType1 | NodeRuntimeStateType2 | NodeRuntimeStateType3]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> (
    NodeRuntimeStateType0
    | NodeRuntimeStateType1
    | NodeRuntimeStateType2
    | NodeRuntimeStateType3
    | None
):
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        NodeRuntimeStateType0 | NodeRuntimeStateType1 | NodeRuntimeStateType2 | NodeRuntimeStateType3
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
