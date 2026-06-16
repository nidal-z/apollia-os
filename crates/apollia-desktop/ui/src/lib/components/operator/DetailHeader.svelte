<script lang="ts">
  /**
   * DetailHeader - canonical header for the detail pane of split screens.
   *
   * Split screens (Connections, Projects, Agents, Tasks split, Memory) carry
   * their screen identity in the topbar breadcrumb, so the detail pane uses this
   * header instead of a full-width PageHeader: a leading visual, a title with
   * inline badges, a one-line meta, and right-aligned actions. A TabBar usually
   * sits directly below it.
   */
  import type { Snippet } from "svelte";

  interface Props {
    /** Leading visual (icon chip / avatar). */
    leading?: Snippet;
    title: string;
    /** One-line context/meta rendered under the title. */
    meta?: string;
    /** Inline badges rendered next to the title. */
    badges?: Snippet;
    /** Full-width row rendered under the title block (e.g. a wrap of badges). */
    footer?: Snippet;
    /** Right-aligned action buttons. */
    actions?: Snippet;
    /** Optional data-testid forwarded to the title element. */
    titleTestid?: string;
    class?: string;
  }

  let { leading, title, meta, badges, footer, actions, titleTestid, class: className = "" }: Props =
    $props();
</script>

<div class="px-8 pt-6 pb-4 border-b border-border/60 {className}">
  <div class="flex items-center gap-3.5">
    {#if leading}
      <div class="shrink-0">{@render leading()}</div>
    {/if}
    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-2">
        <h2
          class="m-0 truncate text-[18px] font-semibold text-foreground"
          style="letter-spacing: -0.3px;"
          data-testid={titleTestid}
        >
          {title}
        </h2>
        {#if badges}{@render badges()}{/if}
      </div>
      {#if meta}
        <p class="mt-0.5 truncate text-[11.5px] text-muted-foreground">{meta}</p>
      {/if}
    </div>
    {#if actions}
      <div class="flex shrink-0 items-center gap-2">{@render actions()}</div>
    {/if}
  </div>
  {#if footer}
    <div class="mt-3.5 flex flex-wrap items-center gap-2">{@render footer()}</div>
  {/if}
</div>
