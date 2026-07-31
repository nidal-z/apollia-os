# Customize the MCP catalogue

> For power users and team administrators who want to add, disable or modify MCP catalogue entries, without waiting for an Apollia release.

## Prerequisites

- You know how to edit a JSON file.
- You have a text editor.
- Apollia must be closed while you edit (v0.1.0 does no hot-reload).

## The `mcp-overrides.json` file

Path: `~/.apollia/mcp-overrides.json`.

Format: a JSON object with three optional keys, applied in this order:

1. **`disable`**: remove entries from the catalogue.
2. **`override`**: patch existing entries (deep merge).
3. **`add`**: add new entries.

## Steps

1. Close Apollia Desktop.
2. Create or edit `~/.apollia/mcp-overrides.json` following the use cases below.
3. Save.
4. Restart Apollia Desktop.
5. Open **Connections, + Discover** and check the result.

## Use cases

### Hide an entry

```json
{ "disable": ["@modelcontextprotocol/server-puppeteer"] }
```

The entry disappears from the catalogue. Already installed servers are not uninstalled.

### Modify an existing entry (deep merge)

```json
{
  "override": {
    "@notionhq/notion-mcp-server": {
      "default_requires_approval": false
    }
  }
}
```

The patch is applied by recursive merge. Objects are merged, scalars and arrays are replaced entirely.

### Add your own entry

```json
{
  "add": [
    {
      "package_identifier": "@local/mon-mcp",
      "operator_label": { "fr": "Mon MCP", "en": "My MCP" },
      "description": { "fr": "Serveur interne de mon équipe." },
      "category": "internal",
      "icon_name": "building",
      "trust_level": "custom",
      "default_requires_approval": true,
      "remote_url": "https://mcp.interne.example",
      "remote_transport": "streamable-http",
      "cost_model": { "kind": "free" }
    }
  ]
}
```

The required fields are `package_identifier`, `operator_label`, `category`, `icon_name`, `trust_level`. The full schema (every available field) is documented in the technical reference.

## Verification

- The entries in `disable` are no longer visible under **+ Discover**.
- The entries in `add` appear with their logo and a `Custom` badge.
- Overrides are reflected (for example `default_requires_approval=false` makes the tools auto-approved).
- If you want to confirm from the logs, look at `~/.apollia/logs/runtime.log`, line `mcp.catalog.overrides.applied`.

## If it does not work

- **The file looks ignored**: it is probably malformed (invalid JSON). Apollia logs a `mcp.catalog.overrides.parse_failed` warning but does not crash. Validate with `jq . ~/.apollia/mcp-overrides.json`.
- **An `add` entry does not appear**: a required field is missing. The other entries in the file are still applied.
- **You want to reload without restarting**: not supported in v0.1.0, a restart is required.

## v0.1.0 limitations

- No hot-reload.
- No cryptographic validation on `add` entries (you are responsible for the content).
- No multi-user governance (PR review), planned for v0.3 with an optional remote registry.

> **Technical reference:** [Apollia reference](/reference) , full `ConnectorEnrichment` schema, application order, edge cases.
