<script lang="ts">
  /**
   * PageLayout — canonical route container.
   *
   * Enforces the Dashboard/Chat/Agents reference layout:
   *   • `mx-auto w-full max-w-6xl` centred container
   *   • `<PageHeader>` with kicker, title, subtitle, actions snippet
   *   • Default content padding handled by individual sections inside `children`
   *     (PageHeader already owns its `px-8 pt-10 pb-5` padding).
   *
   * Routes that need a custom layout (split panes, sidebars, full-width tab
   * shells) opt out by NOT using this primitive and assembling their own
   * containers. Examples : Chat (tripartite), Agents (list/detail), Settings
   * (lazy nav).
   */
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";
  import PageHeader from "./PageHeader.svelte";

  interface Props {
    /** Uppercase mono kicker text rendered above the title. */
    kicker?: string;
    /** Main h1. */
    title: string;
    /** Secondary muted-foreground prose. */
    subtitle?: string;
    /** Right-aligned action area in the header. */
    actions?: Snippet;
    /** Width cap. Override only when the route needs to break the 6xl rule. */
    maxWidth?: "max-w-4xl" | "max-w-5xl" | "max-w-6xl" | "max-w-7xl";
    /** Optional custom class on the container. */
    class?: string;
    /** Optional test id forwarded to the container. */
    "data-testid"?: string;
    children: Snippet;
  }

  let {
    kicker,
    title,
    subtitle,
    actions,
    maxWidth = "max-w-6xl",
    class: className = "",
    "data-testid": dataTestId,
    children,
  }: Props = $props();
</script>

<div
  class={cn("mx-auto w-full", maxWidth, className)}
  data-testid={dataTestId}
>
  <PageHeader {kicker} {title} {subtitle} {actions} />
  {@render children()}
</div>
