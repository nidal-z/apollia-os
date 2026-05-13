<script lang="ts">
  /**
   * ModeChip — pill-shaped selector at the top of the sidebar.
   *
   * Reads the current UI mode, exposes a 2-item dropdown (Operator / Builder)
   * with a short description of each so a new user learns the distinction
   * inline, without a visit to the docs.
   */
  import { t } from "svelte-i18n";
  import { Popover } from "$lib/components/ui/popover";
  import { uiMode, type UIMode } from "$lib/stores/mode";
  import { Check, ChevronDown } from "lucide-svelte";

  interface Props {
    collapsed: boolean;
  }

  let { collapsed }: Props = $props();

  let open = $state(false);

  const current = $derived($uiMode);
  const shortLabel = $derived(current === "operator" ? "OP" : "BLD");
  const toneClass = $derived(
    current === "operator"
      ? "bg-primary/15 text-primary ring-primary/30"
      : "bg-secondary/20 text-secondary ring-secondary/40",
  );

  function pick(mode: UIMode) {
    uiMode.set(mode);
    open = false;
  }
</script>

<Popover bind:open align="start" side="bottom">
  {#snippet trigger(triggerProps: Record<string, unknown>)}
    <button
      {...triggerProps}
      class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-semibold ring-1 transition-colors hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 {toneClass}"
      class:px-1.5={collapsed}
      aria-label={$t("sidebar.mode.chip_aria", {
        values: { mode: $t(`sidebar.mode.${current}.label`) },
      })}
      title={$t("sidebar.mode.chip_tooltip", {
        values: {
          current: $t(`sidebar.mode.${current}.label`),
          other: $t(
            `sidebar.mode.${current === "operator" ? "builder" : "operator"}.label`,
          ),
        },
      })}
      data-testid="sidebar-mode-chip"
      data-mode={current}
    >
      <span>{shortLabel}</span>
      {#if !collapsed}
        <ChevronDown size={12} strokeWidth={2} aria-hidden="true" />
      {/if}
    </button>
  {/snippet}

  {#snippet content()}
    <div role="menu" class="flex w-60 flex-col gap-1" data-testid="sidebar-mode-menu">
      {#each ["operator", "builder"] as const as m}
        <button
          type="button"
          role="menuitemradio"
          aria-checked={current === m}
          class="flex items-start gap-2 rounded-md px-2 py-2 text-left text-sm transition-colors hover:bg-muted focus-visible:outline-none focus-visible:bg-muted"
          data-testid="sidebar-mode-option-{m}"
          onclick={() => pick(m)}
        >
          <span class="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center">
            {#if current === m}
              <Check size={14} strokeWidth={2.25} class="text-primary" />
            {/if}
          </span>
          <span class="flex flex-col">
            <span class="font-medium text-foreground">
              {$t(`sidebar.mode.${m}.label`)}
            </span>
            <span class="text-xs text-muted-foreground">
              {$t(`sidebar.mode.${m}.description`)}
            </span>
          </span>
        </button>
      {/each}
    </div>
  {/snippet}
</Popover>
