<script lang="ts">
  import { cn } from "$lib/utils";
  import { fly } from "svelte/transition";
  import { X, CheckCircle, AlertCircle, Info, Loader2 } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { rm } from "$lib/design/motion";
  import type { ToastVariant } from "./store";

  interface Props {
    message: string;
    variant?: ToastVariant;
    visible?: boolean;
    ondismiss?: () => void;
    autoDismiss?: number;
    showProgress?: boolean;
    actionLabel?: string;
    onaction?: () => void;
    "data-testid"?: string;
  }

  let {
    message,
    variant = "info",
    visible = $bindable(true),
    ondismiss,
    autoDismiss,
    showProgress = false,
    actionLabel,
    onaction,
    ...restProps
  }: Props = $props();

  const resolvedDuration = $derived(
    autoDismiss ?? (variant === "loading" ? 0 : 4000),
  );

  const iconByVariant = {
    success: CheckCircle,
    error: AlertCircle,
    info: Info,
    loading: Loader2,
    urgent: AlertCircle,
    warning: AlertCircle,
  } as const;

  const iconClassByVariant: Record<ToastVariant, string> = {
    success: "text-emerald-600 dark:text-emerald-400",
    error: "text-destructive",
    info: "text-sky-600 dark:text-sky-400",
    loading: "text-primary animate-spin",
    urgent: "text-destructive",
    warning: "text-amber-600 dark:text-amber-400",
  };

  const progressColorByVariant: Record<ToastVariant, string> = {
    success: "rgb(16 185 129)",
    error: "hsl(var(--destructive))",
    info: "rgb(14 165 233)",
    loading: "hsl(var(--primary))",
    urgent: "hsl(var(--destructive))",
    warning: "rgb(217 119 6)",
  };

  const isUrgent = $derived(variant === "urgent");
  const ariaRole = $derived(
    variant === "error" || variant === "urgent" ? "alert" : "status",
  );
  const ariaLive = $derived(
    variant === "error" || variant === "urgent" ? "assertive" : "polite",
  );

  let paused = $state(false);
  let remaining = $state(0);
  let startedAt = $state(0);

  $effect(() => {
    if (!visible || resolvedDuration <= 0 || paused) return;
    const duration = remaining > 0 ? remaining : resolvedDuration;
    startedAt = Date.now();
    const timer = setTimeout(() => {
      visible = false;
      ondismiss?.();
    }, duration);
    return () => clearTimeout(timer);
  });

  function handlePause() {
    if (resolvedDuration <= 0 || paused) return;
    const elapsed = Date.now() - startedAt;
    remaining = Math.max(0, (remaining > 0 ? remaining : resolvedDuration) - elapsed);
    paused = true;
  }

  function handleResume() {
    if (!paused) return;
    paused = false;
  }

  function handleDismiss() {
    visible = false;
    ondismiss?.();
  }

  function handleAction() {
    onaction?.();
    handleDismiss();
  }

  const IconComponent = $derived(iconByVariant[variant]);
  const iconClass = $derived(iconClassByVariant[variant]);
  const progressColor = $derived(progressColorByVariant[variant]);
</script>

{#if visible}
  <div
    class={cn(
      "pointer-events-auto relative flex items-center gap-3 overflow-hidden rounded-lg border border-border bg-card px-4 py-3 shadow-lg",
      isUrgent && "bg-destructive/10 border-destructive/40",
    )}
    role={ariaRole}
    aria-live={ariaLive}
    aria-atomic="true"
    transition:fly={rm({ y: -8, duration: 200 })}
    onmouseenter={handlePause}
    onmouseleave={handleResume}
    onfocusin={handlePause}
    onfocusout={handleResume}
    {...restProps}
  >
    <IconComponent size={18} class={iconClass} aria-hidden="true" />
    <p class="flex-1 text-sm text-card-foreground">{message}</p>
    {#if actionLabel}
      <button
        class="shrink-0 rounded-md px-2.5 py-1 text-xs font-medium text-secondary hover:bg-secondary/10 transition-colors"
        onclick={handleAction}
        data-testid="toast-action"
      >
        {actionLabel}
      </button>
    {/if}
    {#if variant !== "loading"}
      <button
        class="rounded-md p-0.5 text-muted-foreground hover:text-foreground transition-colors"
        onclick={handleDismiss}
        aria-label={$t("toast.dismiss")}
      >
        <X size={14} />
      </button>
    {/if}

    {#if showProgress && resolvedDuration > 0}
      <span
        class="toast-progress"
        style="--toast-duration: {resolvedDuration}ms; --toast-progress-color: {progressColor}; animation-play-state: {paused ? 'paused' : 'running'};"
        aria-hidden="true"
      ></span>
    {/if}
  </div>
{/if}

<style>
  .toast-progress {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    width: 100%;
    background-color: var(--toast-progress-color);
    transform-origin: left center;
    animation: toast-progress var(--toast-duration) linear forwards;
  }

  @media (prefers-reduced-motion: reduce) {
    .toast-progress {
      animation: none;
      transform: scaleX(1);
    }
  }
</style>
