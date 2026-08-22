---
sidebar_position: 8
title: ctx.secrets
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.secrets`

Service type: `SecretsInterface` (from `apollia.context.secrets`).

### `SecretsInterface`

_Bases: Protocol_

Read-only access to credentials declared in ``@agent(secrets=(...))``.

Secrets are resolved at task startup by ``apollia-auth`` (keyring +
OAuth refresh).  Agents never write to this surface - credentials are
provisioned from the desktop app (Settings > Integrations) or with
``apollia-os tools credentials set``.

#### `get`

```python
def get(self, key: str) -> str | None
```

Returns the secret value, or ``None`` if not configured.

#### `has`

```python
def has(self, key: str) -> bool
```

Whether ``key`` resolved to a value at task startup.

#### `list_names`

```python
def list_names(self) -> list[str]
```

Return the secret keys the manifest declares, never their values.
