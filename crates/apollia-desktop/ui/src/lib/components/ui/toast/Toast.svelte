<script lang="ts">
  /**
   * Transient notification. Six variants; the progress bar drains over the
   * dismiss window and pauses on hover / focus. Error + urgent escalate to
   * `role="alert"` / `aria-live="assertive"`. Colors, elevation and the
   * progress tint all resolve from design tokens (no raw palette).
   */
  import { cn } from "$lib/utils";
  import { fly } from "svelte/transition";
  import { X, CheckCircle, AlertCircle, Info, AlertTriangle, Loader2 } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { rm } from "$lib/design/motion";
  import type { ToastVariant } from "./store";

  interface Props {
    message: string;
    description?: string;
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
    description,
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
    warning: AlertTriangle,
  } as const;

  const iconClassByVariant: Record<ToastVariant, string> = {
    success: "text-success",
    error: "text-destructive",
    info: "text-info",
    loading: "text-primary animate-spin",
    urgent: "text-destructive",
    warning: "text-warning",
  };

  const isTinted = $derived(variant === "error" || variant === "urgent");
  const ariaRole = $derived(isTinted ? "alert" : "status");
  const ariaLive = $derived(isTinted ? "assertive" : "polite");

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
    remaining = Math.max(
      0,
      (remaining > 0 ? remaining : resolvedDuration) - elapsed,
    );
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
</script>

{#if visible}
  <div
    class={cn(
      "toast pointer-events-auto relative flex w-full items-center gap-3 overflow-hidden rounded-lg border border-border bg-card px-3.5 py-3 shadow-elev-2",
      isTinted && "border-destructive/40 bg-destructive/[0.08]",
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
    <IconComponent size={18} class={cn("shrink-0", iconClass)} aria-hidden="true" />
    <div class="min-w-0 flex-1">
      <p class="text-sm text-card-foreground">{message}</p>
      {#if description}
        <p class="desc text-muted-foreground">{description}</p>
      {/if}
    </div>
    {#if actionLabel}
      <Button
        variant="ghost"
        size="sm"
        class="shrink-0 px-2.5 text-primary hover:bg-primary/10"
        onclick={handleAction}
        data-testid="toast-action"
      >
        {actionLabel}
      </Button>
    {/if}
    {#if variant !== "loading"}
      <Button
        variant="ghost"
        size="icon-sm"
        class="shrink-0 text-muted-foreground hover:text-foreground"
        onclick={handleDismiss}
        aria-label={$t("toast.dismiss")}
      >
        <X size={14} aria-hidden="true" />
      </Button>
    {/if}

    {#if showProgress && resolvedDuration > 0}
      <span
        class={cn("progress", iconClass)}
        style="--toast-duration: {resolvedDuration}ms; animation-play-state: {paused
          ? 'paused'
          : 'running'};"
        aria-hidden="true"
      ></span>
    {/if}
  </div>
{/if}

<style>
  .desc {
    margin-top: 0.125rem;
    font-size: 0.71875rem;
  }

  .progress {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 0.125rem;
    width: 100%;
    background-color: currentColor;
    transform-origin: left center;
    animation: toast-progress var(--toast-duration) linear forwards;
  }

  @keyframes toast-progress {
    from {
      transform: scaleX(1);
    }
    to {
      transform: scaleX(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .progress {
      animation: none;
      transform: scaleX(1);
    }
  }
</style>
