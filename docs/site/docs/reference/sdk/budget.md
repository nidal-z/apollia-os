---
sidebar_position: 15
title: ctx.budget
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.budget`

Service type: `BudgetView` (from `apollia.context.budget`).

### `BudgetView`

_Bases: Protocol_

Runtime step budget tracking, read-only from the agent's perspective.

The actual enforcement happens in the Rust runtime (StepBudget actor).
This view lets agents introspect remaining budget without bypassing
the non-negotiable guard-rails (Principle 7).

| Field | Type | Default |
| --- | --- | --- |
| `steps_remaining` | `int` |  |
| `tool_calls_remaining` | `int` |  |
| `elapsed_seconds` | `float` |  |
| `wall_clock_remaining` | `float \| None` |  |
