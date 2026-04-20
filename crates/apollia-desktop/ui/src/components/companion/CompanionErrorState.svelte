<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    AlertTriangle,
    ChevronDown,
    ChevronRight,
    Copy,
    Lock,
    RotateCcw,
    Settings,
    WifiOff,
    X,
  } from "lucide-svelte";
  import type {
    CompanionError,
    CompanionErrorKind,
  } from "$lib/stores/companion";

  interface Props {
    error: CompanionError;
    onRetry: () => void;
    onOpenSettings: () => void;
    onClose: () => void;
  }

  let { error, onRetry, onOpenSettings, onClose }: Props = $props();

  let detailsOpen = $state(false);
  let codeCopied = $state(false);

  const iconFor: Record<CompanionErrorKind, typeof WifiOff> = {
    network: WifiOff,
    permissions: Lock,
    timeout: AlertTriangle,
    internal: AlertTriangle,
  };

  const Icon = $derived(iconFor[error.kind]);

  const settingsTarget = $derived.by(() => {
    switch (error.kind) {
      case "network":
        return "/settings/integrations";
      case "permissions":
        return "/settings/permissions";
      default:
        return "/settings/llm";
    }
  });

  async function copyCode() {
    try {
      await navigator.clipboard.writeText(error.code);
      codeCopied = true;
      setTimeout(() => (codeCopied = false), 1500);
    } catch {
      // Clipboard unavailable — silent fallback.
    }
  }
</script>

<div
  class="flex h-full flex-col gap-4 overflow-y-auto p-5 text-sm"
  role="alert"
  data-testid="companion-error-state"
  data-error-kind={error.kind}
  data-error-code={error.code}
>
  <div class="flex items-start gap-3">
    <span
      class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-destructive/10 text-destructive"
    >
      <Icon size={20} />
    </span>
    <div class="min-w-0 flex-1">
      <h3 class="text-base font-semibold text-foreground">
        {$t(`companion.error.${error.kind}.title`)}
      </h3>
      <p class="mt-1 text-xs leading-relaxed text-muted-foreground">
        {$t(`companion.error.${error.kind}.message`)}
      </p>
    </div>
  </div>

  <div>
    <p class="text-xs font-medium text-foreground">
      {$t("companion.error.possible_causes")}
    </p>
    <ul class="mt-1 list-disc space-y-1 pl-5 text-xs text-muted-foreground">
      <li>{$t(`companion.error.${error.kind}.cause_1`)}</li>
      <li>{$t(`companion.error.${error.kind}.cause_2`)}</li>
      <li>{$t(`companion.error.${error.kind}.cause_3`)}</li>
    </ul>
  </div>

  <div class="flex items-center gap-2 rounded-md border border-border bg-muted/40 px-2 py-1.5">
    <code
      class="flex-1 truncate font-mono text-[11px] text-foreground"
      data-testid="companion-error-code"
    >
      {error.code}
    </code>
    <button
      type="button"
      class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      onclick={copyCode}
      aria-label={$t("companion.error.copy_code")}
      data-testid="companion-error-copy-code"
    >
      <Copy size={12} />
    </button>
    {#if codeCopied}
      <span class="text-[10px] text-muted-foreground">
        {$t("companion.error.copied")}
      </span>
    {/if}
  </div>

  {#if error.technicalDetails || error.message}
    <div>
      <button
        type="button"
        class="inline-flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
        onclick={() => (detailsOpen = !detailsOpen)}
        aria-expanded={detailsOpen}
        data-testid="companion-error-details-toggle"
      >
        {#if detailsOpen}
          <ChevronDown size={12} />
        {:else}
          <ChevronRight size={12} />
        {/if}
        {$t("companion.error.technical_details")}
      </button>
      {#if detailsOpen}
        <pre
          class="mt-1 max-h-40 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-[10px] leading-snug text-muted-foreground"
          data-testid="companion-error-stack"
        >{error.technicalDetails ?? error.message}</pre>
      {/if}
    </div>
  {/if}

  <div class="mt-auto flex flex-col gap-2 pt-2">
    <button
      type="button"
      class="inline-flex w-full items-center justify-center gap-2 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
      onclick={onRetry}
      data-testid="companion-error-retry"
    >
      <RotateCcw size={14} />
      {$t("companion.error.retry")}
    </button>
    <button
      type="button"
      class="inline-flex w-full items-center justify-center gap-2 rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground transition-colors hover:bg-muted"
      onclick={onOpenSettings}
      data-testid="companion-error-settings"
      data-target={settingsTarget}
    >
      <Settings size={14} />
      {$t("companion.error.open_settings")}
    </button>
    <button
      type="button"
      class="inline-flex w-full items-center justify-center gap-2 rounded-md px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      onclick={onClose}
      data-testid="companion-error-close"
    >
      <X size={12} />
      {$t("companion.error.close_companion")}
    </button>
  </div>
</div>
