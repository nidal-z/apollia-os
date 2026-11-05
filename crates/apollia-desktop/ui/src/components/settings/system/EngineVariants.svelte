<script lang="ts">
  /**
   * EngineVariants - System > Engines: every variant of the inference engine
   * the followed channel offers for this platform, installed or not, each with
   * the verdict of the runtime on this machine and its reason. The variants
   * that cannot run here are folded behind one button. Before the channel was
   * ever read the list says so and offers the check.
   */
  import { t } from "svelte-i18n";
  import { Layers, RefreshCw } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import SettingsSection from "../SettingsSection.svelte";
  import { variantLabel } from "./EngineList.svelte";
  import type { EngineVerdict, EnginesResponse } from "$lib/ipc/system";
  import {
    VERDICT_KEYS,
    incompatibleCount,
    reasonSentence,
    shownVariants,
    variantList,
    type VariantRow,
  } from "$lib/settings/engineVariants";

  interface Props {
    response: EnginesResponse | null;
    /** The system the application runs on, as `get_system_info` states it. */
    os: string | null;
    /** True while the channel is checked. */
    checking: boolean;
    oncheck: () => void;
  }

  let { response, os, checking, oncheck }: Props = $props();

  const list = $derived(variantList(response, os));
  const rows = $derived(list.kind === "rows" ? list.rows : []);
  let showIncompatible = $state(false);
  const shown = $derived(shownVariants(rows, showIncompatible));
  const hidden = $derived(incompatibleCount(rows));

  const BADGES: Record<EngineVerdict, "success" | "neutral" | "danger"> = {
    recommended: "success",
    compatible: "neutral",
    incompatible: "danger",
  };

  function name(row: VariantRow): string {
    return variantLabel(row.variant, $t("settings.engines.variant_cpu")) ?? row.variant;
  }

  function reason(row: VariantRow): string | null {
    if (!row.reason) return null;
    const sentence = reasonSentence(row.reason, row.verdict);
    return $t(sentence.key, { values: sentence.values });
  }
</script>

<SettingsSection
  title={$t("settings.engines.variants_title")}
  description={$t("settings.engines.variants_desc")}
  bodyLayout="flush"
  data-testid="engine-variant-list"
>
  {#snippet icon()}<Layers size={15} strokeWidth={1.75} />{/snippet}

  {#if list.kind === "unread"}
    <div class="flex flex-wrap items-center gap-3 py-3" data-testid="engine-variant-unread">
      <p class="min-w-0 flex-1 text-body-sm text-muted-foreground">
        {$t("settings.engines.variants_unread")}
      </p>
      <Button
        variant="outline"
        size="sm"
        loading={checking}
        disabled={!response?.verifiable}
        onclick={oncheck}
        data-testid="engine-variant-check"
      >
        {#snippet icon()}<RefreshCw size={13} />{/snippet}
        {$t("settings.engines.variants_check")}
      </Button>
    </div>
  {:else if list.kind === "none"}
    <p class="py-3 text-body-sm text-muted-foreground" data-testid="engine-variant-none">
      {$t("settings.engines.variants_none")}
    </p>
  {:else}
    <ul class="divide-y divide-border/50">
      {#each shown as row (row.variant)}
        <li class="flex flex-col gap-1 py-3" data-testid="engine-variant-row-{row.variant}">
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-body-sm font-medium text-foreground">llama.cpp · {name(row)}</span>
            <Badge variant="neutral" outline size="sm">{row.tag}</Badge>
            {#if row.verdict}
              <Badge
                variant={BADGES[row.verdict]}
                outline={row.verdict !== "recommended"}
                size="sm"
                data-testid="engine-variant-row-{row.variant}-verdict"
              >
                {$t(VERDICT_KEYS[row.verdict])}
              </Badge>
            {/if}
            {#if row.experimental}
              <Badge variant="warning" outline size="sm" data-testid="engine-variant-row-{row.variant}-experimental">
                {$t("settings.engines.variant_experimental_windows")}
              </Badge>
            {/if}
            {#if row.installed}
              <Badge variant="info" outline size="sm" data-testid="engine-variant-row-{row.variant}-installed">
                {$t("settings.engines.variant_installed")}
              </Badge>
            {/if}
          </div>
          {#if reason(row)}
            <p
              class="text-caption {row.verdict === 'recommended' ? 'text-warning' : 'text-muted-foreground'}"
              data-testid="engine-variant-row-{row.variant}-reason"
            >
              {reason(row)}
            </p>
          {/if}
        </li>
      {/each}
    </ul>
    {#if hidden > 0}
      <div class="pt-2">
        <Button
          variant="ghost"
          size="sm"
          onclick={() => (showIncompatible = !showIncompatible)}
          aria-expanded={showIncompatible}
          data-testid="engine-variant-toggle-incompatible"
        >
          {showIncompatible
            ? $t("settings.engines.variants_hide_incompatible")
            : $t("settings.engines.variants_show_incompatible", { values: { count: hidden } })}
        </Button>
      </div>
    {/if}
  {/if}
</SettingsSection>
