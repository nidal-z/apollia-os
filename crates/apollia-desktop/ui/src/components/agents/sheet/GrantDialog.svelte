<script lang="ts">
  /**
   * GrantDialog - grants the agent a tool, another agent or a workflow. Each
   * candidate already granted shows checked and stays so; the button says how
   * many accesses the choice adds. Nothing is written until the sheet is saved.
   */
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import {
    filterCandidates,
    GRANT_GROUP_LABEL,
    type GrantCandidate,
    type GrantKind,
  } from "./agentAdjust";

  interface Props {
    open: boolean;
    /** The accesses of the draft. */
    granted: string[];
    loadCandidates: () => Promise<GrantCandidate[]>;
    onclose: () => void;
    onconfirm: (ids: string[]) => void;
  }

  let { open, granted, loadCandidates, onclose, onconfirm }: Props = $props();

  const KINDS: readonly GrantKind[] = ["tool", "agent", "workflow"];

  let candidates = $state<GrantCandidate[]>([]);
  let loading = $state(false);
  let query = $state("");
  let picked = $state<string[]>([]);

  $effect(() => {
    if (!open) return;
    query = "";
    picked = [];
    loading = true;
    let live = true;
    void loadCandidates()
      .then((list) => {
        if (live) candidates = list;
      })
      .catch(() => {
        if (live) candidates = [];
      })
      .finally(() => {
        if (live) loading = false;
      });
    return () => {
      live = false;
    };
  });

  const shown = $derived(filterCandidates(candidates, query));

  function toggle(id: string, on: boolean): void {
    picked = on ? [...picked, id] : picked.filter((p) => p !== id);
  }
</script>

<Dialog {open} {onclose} size="md" title={$t("agent_adjust.grant_title")} data-testid="agent-adjust-grant-dialog">
  <p class="m-0 mb-3 text-caption text-muted-foreground">{$t("agent_adjust.grant_legend")}</p>
  <Input
    bind:value={query}
    placeholder={$t("agent_adjust.grant_search")}
    aria-label={$t("agent_adjust.grant_search")}
    data-testid="agent-adjust-grant-search"
  />
  <div class="mt-3 max-h-80 space-y-3 overflow-y-auto">
    {#if loading}
      <p class="m-0 text-body-xs text-muted-foreground">{$t("common.loading")}</p>
    {:else if shown.length === 0}
      <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-adjust-grant-empty">
        {$t("agent_adjust.grant_empty")}
      </p>
    {:else}
      {#each KINDS as kind (kind)}
        {@const rows = shown.filter((c) => c.kind === kind)}
        {#if rows.length > 0}
          <div data-testid="agent-adjust-grant-group" data-kind={kind}>
            <div class="text-body-xs font-semibold text-foreground">{$t(GRANT_GROUP_LABEL[kind])}</div>
            <ul class="m-0 mt-1 list-none space-y-1 p-0">
              {#each rows as row (row.id)}
                {@const already = granted.includes(row.id)}
                <li class="flex items-start gap-2 py-1" data-testid="agent-adjust-grant-row" data-access={row.id}>
                  <Checkbox
                    checked={already || picked.includes(row.id)}
                    disabled={already}
                    onchange={(on) => toggle(row.id, on)}
                    data-testid="agent-adjust-grant-check"
                  />
                  <div class="min-w-0 flex-1">
                    <div class="text-body-xs text-foreground">{row.name}</div>
                    <div class="text-caption text-muted-foreground">
                      {already ? $t("agent_adjust.grant_already") : row.description}
                    </div>
                  </div>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      {/each}
    {/if}
  </div>
  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="agent-adjust-grant-cancel">{$t("common.cancel")}</Button>
    <Button
      variant="primary-solid"
      disabled={picked.length === 0}
      onclick={() => onconfirm(picked)}
      data-testid="agent-adjust-grant-confirm"
    >
      {$t("agent_adjust.grant_confirm", { values: { count: picked.length } })}
    </Button>
  </DialogFooter>
</Dialog>
