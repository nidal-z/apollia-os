---
sidebar_position: 0
title: How-to guides
description: "Task-oriented recipes: install and run the runtime, write tools and workflows, test and package them, embed Apollia, and keep a human in the loop."
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

## Build packages

- [Write a tool](/how-to/write-a-worker): a Python function an agent calls,
  declared with `@tool` and granted in the manifest of the package.
- [Write a workflow](/how-to/write-a-director): steps you write in Python, each
  handing work to an agent, to another workflow or to the user.
- [Make an agent plan first](/how-to/run-an-orchestrated-agent): have an agent
  write the steps of a task before it acts.
- [Test your tools and workflows](/how-to/test-your-agents): unit-test them
  against a mock context, then evaluate whole tasks against a live runtime.
- [Package and distribute](/how-to/package-and-distribute-an-agent): build,
  sign, install and share a `.apo` archive.

## Integrate and operate

- [Integrate via the driving contract](/how-to/integrate-via-driving-contract):
  control the runtime from a host application over the HTTP API.
- [Embed via federation](/how-to/embed-via-federation): expose your product as
  MCP tools and let an agent write back through your REST API.
- [Set up a Google OAuth client](/how-to/set-up-a-google-oauth-client): register
  your own client in the Google Cloud console, click by click, so the Gmail,
  Calendar and Drive connector works. Microsoft 365 needs no equivalent.
- [Keep a human in the loop](/how-to/human-in-the-loop): stop a workflow on a
  question, and know what asks for an approval on its own.
- [Audit and verify a run](/how-to/audit-and-verify): read the
  trail and verify the journal.
- [Deploy in production](/how-to/deploy-in-production): run Apollia as a managed
  service with TCP, authentication, and TLS.
