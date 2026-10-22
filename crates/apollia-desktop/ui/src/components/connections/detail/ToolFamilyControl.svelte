<script lang="ts">
  /**
   * ToolFamilyControl - the family of one tool of a connection, in plain
   * words, and for a tool of a service added outside the selection, "Change":
   * the families to choose from, the choice given by the runtime and recorded
   * in the journal.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { messageOf } from "$lib/connections/errors";
  import {
    RECLASSIFY_FAMILIES,
    canReclassify,
    familyLabel,
    localToolName,
  } from "$lib/connections/connectionSections";
  import { setConnectionToolFamily } from "$lib/ipc/connections";
  import { isSensitiveFamily, type ActionFamily } from "$lib/tools/action-family";
  import type { Connection } from "$lib/types";

  interface Props {
    connection: Connection;
    /** The tool, by the identifier the connection lists it under. */
    toolId: string;
    /** The family was changed: the connection as the list now shows it. */
    onupdated?: (connection: Connection) => void;
  }

  let { connection, toolId, onupdated }: Props = $props();

  let editing = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let choice = $state<string>("");

  const tool = $derived(connection.tools.find((item) => item.id === toolId) ?? null);
  const local = $derived(localToolName(toolId));
  const changeable = $derived(tool !== null && onupdated !== undefined && canReclassify(connection, tool));

  $effect(() => {
    void toolId;
    void connection.id;
    editing = false;
    error = null;
  });

  function open(): void {
    choice = tool?.family ?? "";
    error = null;
    editing = true;
  }

  async function apply(): Promise<void> {
    if (!tool || choice === "" || choice === tool.family) {
      editing = false;
      return;
    }
    busy = true;
    error = null;
    try {
      const updated = await setConnectionToolFamily(connection.id, local, choice as ActionFamily);
      editing = false;
      onupdated?.(updated);
    } catch (err: unknown) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if tool}
  <span class="inline-flex flex-wrap items-center gap-1.5">
    {#if editing}
      <Select size="sm" bind:value={choice} disabled={busy} data-testid="tool-family-select-{local}">
        {#each RECLASSIFY_FAMILIES as family (family)}
          <option value={family}>{$t(familyLabel(family))}</option>
        {/each}
      </Select>
      <Button variant="primary-solid" size="sm" onclick={() => void apply()} loading={busy} data-testid="tool-family-apply-{local}">
        {$t("connections.card.tools.apply")}
      </Button>
      <Button variant="ghost" size="sm" onclick={() => (editing = false)} disabled={busy}>
        {$t("common.cancel")}
      </Button>
    {:else}
      <Badge
        size="sm"
        variant={isSensitiveFamily(tool.family) ? "warning" : "neutral"}
        data-testid="tool-family-{local}"
        data-family={tool.family}
      >
        {$t(familyLabel(tool.family))}
      </Badge>
      {#if changeable}
        <Button variant="ghost" size="sm" onclick={open} data-testid="tool-family-change-{local}">
          {$t("connections.card.tools.change")}
        </Button>
      {/if}
    {/if}
    {#if error}
      <span class="text-caption text-destructive" data-testid="tool-family-error-{local}">{error}</span>
    {/if}
  </span>
{/if}
