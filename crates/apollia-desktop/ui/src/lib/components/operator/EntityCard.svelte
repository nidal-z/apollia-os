<script lang="ts">
  /**
   * EntityCard - canonical shell for a domain "entity" card: a thing with a
   * leading icon, a title, optional badge / meta, an optional trailing control
   * and a footer action cluster. Replaces the hand-rolled containers that each
   * entity card used to reinvent (Notification, Connection, Project, Agent
   * package, LLM backend, tool, permission, config file, transcript, HITL...).
   *
   * Anatomy (all slots optional except the title):
   *   [accent bar] · icon | title + badges + trailing · body · footer actions
   */
  import type { Snippet } from "svelte";
  import { Card } from "$lib/components/ui/card";

  type Tone = "primary" | "info" | "warning" | "success" | "destructive";

  interface Props {
    /** Top accent bar tone. Omit for no bar. */
    accent?: Tone;
    /** Tint of the leading icon container. */
    iconTone?: Tone;
    title?: string;
    subtitle?: string;
    /** Whole-card click (adds the interactive press / lift + cursor). */
    onclick?: (event: MouseEvent) => void;
    icon?: Snippet;
    badges?: Snippet;
    trailing?: Snippet;
    body?: Snippet;
    actions?: Snippet;
    class?: string;
    /** Passthrough for data-* attributes (data-testid and friends). */
    [key: `data-${string}`]: unknown;
  }

  let {
    accent,
    iconTone = "primary",
    title,
    subtitle,
    onclick,
    icon,
    badges,
    trailing,
    body,
    actions,
    class: className = "",
    ...rest
  }: Props = $props();

  const accentClass = $derived(
    accent
      ? {
          primary: "bg-primary/60",
          info: "bg-info/60",
          warning: "bg-warning/60",
          success: "bg-success/60",
          destructive: "bg-destructive/60",
        }[accent]
      : "",
  );
  const iconToneClass = $derived(
    {
      primary: "bg-primary/10 text-primary",
      info: "bg-info/10 text-info",
      warning: "bg-warning/10 text-warning",
      success: "bg-success/10 text-success",
      destructive: "bg-destructive/10 text-destructive",
    }[iconTone],
  );
</script>

<Card
  surface="solid"
  interactive={!!onclick}
  {onclick}
  class="relative flex flex-col overflow-hidden {className}"
  {...rest}
>
  {#if accent}
    <div class="h-0.5 w-full shrink-0 {accentClass}"></div>
  {/if}
  <div class="flex flex-1 flex-col gap-2.5 px-3.5 pt-3 pb-2.5">
    <div class="flex items-center gap-2.5">
      {#if icon}
        <div class="flex size-8 shrink-0 items-center justify-center rounded-lg {iconToneClass}">
          {@render icon()}
        </div>
      {/if}
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          {#if title}<span class="truncate text-[13px] font-medium">{title}</span>{/if}
          {#if badges}{@render badges()}{/if}
        </div>
        {#if subtitle}
          <p class="mt-0.5 truncate text-[11px] text-muted-foreground">{subtitle}</p>
        {/if}
      </div>
      {#if trailing}<div class="shrink-0">{@render trailing()}</div>{/if}
    </div>
    {#if body}{@render body()}{/if}
    {#if actions}
      <div class="mt-auto flex items-center justify-end gap-1 border-t border-border/30 pt-2">
        {@render actions()}
      </div>
    {/if}
  </div>
</Card>
