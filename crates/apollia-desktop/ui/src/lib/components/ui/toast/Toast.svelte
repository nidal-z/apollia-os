<script lang="ts">
  import { cn } from "$lib/utils";
  import { fly } from "svelte/transition";
  import { X, CheckCircle, AlertCircle, Info } from "lucide-svelte";
  import { t } from "svelte-i18n";

  interface Props {
    message: string;
    variant?: "success" | "error" | "info";
    visible?: boolean;
    ondismiss?: () => void;
    autoDismiss?: number;
    actionLabel?: string;
    onaction?: () => void;
    "data-testid"?: string;
  }

  let {
    message,
    variant = "info",
    visible = $bindable(true),
    ondismiss,
    autoDismiss = 4000,
    actionLabel,
    onaction,
    ...restProps
  }: Props = $props();

  const variantConfig = {
    success: { icon: CheckCircle, classes: "text-emerald-600 dark:text-emerald-400" },
    error: { icon: AlertCircle, classes: "text-red-600 dark:text-red-400" },
    info: { icon: Info, classes: "text-sky-600 dark:text-sky-400" },
  };

  $effect(() => {
    if (visible && autoDismiss > 0) {
      const timer = setTimeout(() => {
        visible = false;
        ondismiss?.();
      }, autoDismiss);
      return () => clearTimeout(timer);
    }
  });

  function handleDismiss() {
    visible = false;
    ondismiss?.();
  }

  function handleAction() {
    onaction?.();
    handleDismiss();
  }

  const config = $derived(variantConfig[variant]);
</script>

{#if visible}
  <div
    class={cn(
      "pointer-events-auto flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 shadow-lg",
    )}
    role="alert"
    transition:fly={{ y: -8, duration: 200 }}
    {...restProps}
  >
    {#if variant === "success"}
      <CheckCircle size={18} class={config.classes} />
    {:else if variant === "error"}
      <AlertCircle size={18} class={config.classes} />
    {:else}
      <Info size={18} class={config.classes} />
    {/if}
    <p class="flex-1 text-sm text-card-foreground">{message}</p>
    {#if actionLabel}
      <button
        class="shrink-0 rounded-md px-2.5 py-1 text-xs font-medium text-[#7c5fd6] hover:bg-[#7c5fd6]/10 transition-colors"
        onclick={handleAction}
        data-testid="toast-action"
      >
        {actionLabel}
      </button>
    {/if}
    <button
      class="rounded-md p-0.5 text-muted-foreground hover:text-foreground transition-colors"
      onclick={handleDismiss}
      aria-label={$t("a11y.dismiss")}
    >
      <X size={14} />
    </button>
  </div>
{/if}
