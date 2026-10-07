from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.engine_installation_state_type_0 import EngineInstallationStateType0
from ...models.engine_installation_state_type_1 import EngineInstallationStateType1
from ...models.engine_installation_state_type_2 import EngineInstallationStateType2
from ...models.engine_installation_state_type_3 import EngineInstallationStateType3
from ...models.engine_installation_state_type_4 import EngineInstallationStateType4
from ...models.engine_installation_state_type_5 import EngineInstallationStateType5
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/engines/installation",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> (
    EngineInstallationStateType0
    | EngineInstallationStateType1
    | EngineInstallationStateType2
    | EngineInstallationStateType3
    | EngineInstallationStateType4
    | EngineInstallationStateType5
    | None
):
    if response.status_code == 200:

        def _parse_response_200(
            data: object,
        ) -> (
            EngineInstallationStateType0
            | EngineInstallationStateType1
            | EngineInstallationStateType2
            | EngineInstallationStateType3
            | EngineInstallationStateType4
            | EngineInstallationStateType5
        ):
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_installation_state_type_0 = (
                    EngineInstallationStateType0.from_dict(data)
                )

                return componentsschemas_engine_installation_state_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_installation_state_type_1 = (
                    EngineInstallationStateType1.from_dict(data)
                )

                return componentsschemas_engine_installation_state_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_installation_state_type_2 = (
                    EngineInstallationStateType2.from_dict(data)
                )

                return componentsschemas_engine_installation_state_type_2
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_installation_state_type_3 = (
                    EngineInstallationStateType3.from_dict(data)
                )

                return componentsschemas_engine_installation_state_type_3
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_engine_installation_state_type_4 = (
                    EngineInstallationStateType4.from_dict(data)
                )

                return componentsschemas_engine_installation_state_type_4
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            if not isinstance(data, dict):
                raise TypeError()
            componentsschemas_engine_installation_state_type_5 = (
                EngineInstallationStateType5.from_dict(data)
            )

            return componentsschemas_engine_installation_state_type_5

        response_200 = _parse_response_200(response.json())

        return response_200

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[
    EngineInstallationStateType0
    | EngineInstallationStateType1
    | EngineInstallationStateType2
    | EngineInstallationStateType3
    | EngineInstallationStateType4
    | EngineInstallationStateType5
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
    EngineInstallationStateType0
    | EngineInstallationStateType1
    | EngineInstallationStateType2
    | EngineInstallationStateType3
    | EngineInstallationStateType4
    | EngineInstallationStateType5
]:
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[EngineInstallationStateType0 | EngineInstallationStateType1 | EngineInstallationStateType2 | EngineInstallationStateType3 | EngineInstallationStateType4 | EngineInstallationStateType5]
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
    EngineInstallationStateType0
    | EngineInstallationStateType1
    | EngineInstallationStateType2
    | EngineInstallationStateType3
    | EngineInstallationStateType4
    | EngineInstallationStateType5
    | None
):
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        EngineInstallationStateType0 | EngineInstallationStateType1 | EngineInstallationStateType2 | EngineInstallationStateType3 | EngineInstallationStateType4 | EngineInstallationStateType5
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[
    EngineInstallationStateType0
    | EngineInstallationStateType1
    | EngineInstallationStateType2
    | EngineInstallationStateType3
    | EngineInstallationStateType4
    | EngineInstallationStateType5
]:
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[EngineInstallationStateType0 | EngineInstallationStateType1 | EngineInstallationStateType2 | EngineInstallationStateType3 | EngineInstallationStateType4 | EngineInstallationStateType5]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> (
    EngineInstallationStateType0
    | EngineInstallationStateType1
    | EngineInstallationStateType2
    | EngineInstallationStateType3
    | EngineInstallationStateType4
    | EngineInstallationStateType5
    | None
):
    """
    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        EngineInstallationStateType0 | EngineInstallationStateType1 | EngineInstallationStateType2 | EngineInstallationStateType3 | EngineInstallationStateType4 | EngineInstallationStateType5
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
