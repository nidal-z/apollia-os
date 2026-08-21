---
sidebar_position: 0
title: How-to guides
---

# How-to guides

Task-oriented recipes. Each one solves a single, concrete problem.

## Set up and run

- [Install the desktop app](/how-to/install-the-desktop-app): download a prebuilt
  installer and run the app, no build required.
- [Install and run the runtime](/how-to/install-and-run): build from a checkout
  and get a daemon (and the desktop dev app) running an agent.
- [Get the most from local inference](/how-to/accelerate-local-inference): how the
  embedded `llama-server` engine serves local GGUF models, and how to feed it.

## Build agents

- [Write a director](/how-to/write-a-director): an agent that reasons and drives
  workers.
- [Write a worker](/how-to/write-a-worker): a single-skill agent exposed over
  A2A.
- [Run an orchestrated agent](/how-to/run-an-orchestrated-agent): let the engine
  plan, gate, and verify a multi-step run.
- [Test your agents](/how-to/test-your-agents): unit-test skills against a mock
  context, then integration-test against a live runtime.
- [Package and distribute an agent](/how-to/package-and-distribute-an-agent):
  bundle, install, and share an agent.

## Integrate and operate

- [Integrate via the driving contract](/how-to/integrate-via-driving-contract):
  control the runtime from a host application over the HTTP API.
- [Embed via federation](/how-to/embed-via-federation): expose your product as
  MCP tools and let an agent write back through your REST API.
- [Set up a Google OAuth client](/how-to/set-up-a-google-oauth-client): register
  your own client in the Google Cloud console, click by click, so the Gmail,
  Calendar and Drive connector works. Microsoft 365 needs no equivalent.
- [Keep a human in the loop](/how-to/human-in-the-loop): require approval before
  a consequential action runs.
- [Audit and verify a run](/how-to/audit-and-verify): read the
  trail and verify the journal.
- [Deploy in production](/how-to/deploy-in-production): run Apollia as a managed
  service with TCP, authentication, and TLS.
