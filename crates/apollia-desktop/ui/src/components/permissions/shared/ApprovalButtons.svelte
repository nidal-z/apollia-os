<script lang="ts">
  /**
   * Reusable approval actions cluster (US-SP42-054, finding B.30 —
   * shared with Chantier 3 ApprovalCard).
   *
   * Three variants: Accept (primary), Refuse (ghost + destructive tint),
   * and "Always allow" (outline secondary) with an optional scope
   * dropdown. Stacks vertically below the `sm` breakpoint.
   */
  import { t } from "svelte-i18n";
  import { ChevronDown } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Tooltip } from "bits-ui";
  import AlwaysAcceptScopeChooser from "../AlwaysAcceptScopeChooser.svelte";
  import type { AlwaysAcceptScope } from "$lib/permissions/telemetry";

  interface Props {
    submitting?: boolean;
    showAlways?: boolean;
    defaultScope?: AlwaysAcceptScope;
    onaccept: () => void;
    onrefuse: () => void;
    onalways?: (scope: AlwaysAcceptScope) => void;
    acceptLabel?: string;
    refuseLabel?: string;
    alwaysLabel?: string;
  }

  let {
    submitting = false,
    showAlways = true,
    defaultScope = "session_only",
    onaccept,
    onrefuse,
    onalways,
    acceptLabel,
    refuseLabel,
    alwaysLabel,
  }: Props = $props();

  // eslint-disable-next-line svelte/prefer-writable-derived
  let scope = $state<AlwaysAcceptScope>("session_only");
  $effect(() => {
    scope = defaultScope;
  });
  let scopeOpen = $state(false);
  let confirmAllAlways = $state(false);

  function triggerAlways() {
    if (!onalways) return;
    if (scope === "all_always") {
      confirmAllAlways = true;
      return;
    }
    onalways(scope);
  }

  function confirmAndEmit() {
    confirmAllAlways = false;
    onalways?.(scope);
  }
</script>

<div
  class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-end"
  data-testid="approval-buttons"
>
  {#if showAlways && onalways}
    <div class="flex flex-col gap-1 sm:flex-row sm:items-center sm:gap-1">
      <Tooltip.Root>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="outline"
              size="sm"
              onclick={triggerAlways}
              disabled={submitting}
              class="w-full sm:w-auto"
              data-testid="approval-buttons-always"
            >
              {alwaysLabel ?? $t("permissions.always.button")}
            </Button>
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content side="top">
          <div class="rounded-md bg-foreground px-2 py-1 text-[11px] text-background shadow-lg">
            {$t("permissions.always.tooltip")}
          </div>
        </Tooltip.Content>
      </Tooltip.Root>

      <Button
        variant="ghost"
        size="sm"
        onclick={() => (scopeOpen = !scopeOpen)}
        disabled={submitting}
        aria-expanded={scopeOpen}
        aria-haspopup="true"
        class="sm:!w-9 sm:!px-0"
        data-testid="approval-buttons-scope-toggle"
      >
        <ChevronDown size={14} />
      </Button>
    </div>
  {/if}

  <Button
    variant="ghost"
    size="sm"
    onclick={onrefuse}
    disabled={submitting}
    class="w-full text-destructive hover:bg-destructive/10 sm:w-auto"
    data-testid="approval-buttons-refuse"
  >
    {refuseLabel ?? $t("permissions.actions.refuse")}
  </Button>

  <Button
    variant="success"
    size="lg"
    onclick={onaccept}
    disabled={submitting}
    class="h-10 w-full px-5 text-base shadow-[0_4px_16px_rgba(0,0,0,0.12)] sm:w-auto"
    data-testid="approval-buttons-accept"
  >
    {acceptLabel ?? $t("permissions.actions.accept")}
  </Button>
</div>

{#if scopeOpen && showAlways && onalways}
  <div class="mt-2 rounded-md border border-border bg-card p-3" data-testid="approval-buttons-scope-panel">
    <AlwaysAcceptScopeChooser bind:value={scope} onchange={(s) => (scope = s)} disabled={submitting} />
  </div>
{/if}

{#if confirmAllAlways}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
    data-testid="approval-buttons-all-always-confirm"
  >
    <div class="w-full max-w-md rounded-lg border border-destructive/40 bg-card p-4 shadow-xl">
      <h3 class="text-base font-semibold text-destructive">
        {$t("permissions.always.scope.all_always.confirm_title")}
      </h3>
      <p class="mt-2 text-sm text-muted-foreground">
        {$t("permissions.always.scope.all_always.confirm_body")}
      </p>
      <div class="mt-4 flex justify-end gap-2">
        <Button variant="ghost" size="sm" onclick={() => (confirmAllAlways = false)}>
          {$t("common.cancel")}
        </Button>
        <Button variant="destructive" size="sm" onclick={confirmAndEmit}>
          {$t("permissions.always.scope.all_always.confirm_action")}
        </Button>
      </div>
    </div>
  </div>
{/if}
