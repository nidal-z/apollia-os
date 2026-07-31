---
sidebar_position: 10
title: ctx.logger
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.logger`

Service type: `Logger`, an alias for `logging.Logger` (from `apollia.context.logger`).

ctx.logger - structured logging via stdlib ``logging``.

The runtime configures the actual logger so that records are piped into
the Rust ``tracing`` subscriber.  Agents just use the standard
:class:`logging.Logger` API.
