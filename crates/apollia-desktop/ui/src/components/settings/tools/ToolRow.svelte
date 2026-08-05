<script lang="ts">
  /**
   * ToolRow - one native tool as a discrete governance card.
   *
   * A bordered card (icon tile + title / mono id / status pill + description +
   * optional warning + contract button + configure button + toggle). Richer than the canonical
   * EntityCard because governance needs four visual states (active, disabled,
   * attention, toggling) expressed on the border and the icon tile, plus a
   * right-click context menu of quick governance actions. Toggle and
   * ContextMenu are reused as-is; the card shell is token-only.
   */
  import { t } from "svelte-i18n";
  import {
    AlertTriangle,
    Braces,
    ChevronRight,
    Power,
    Settings2,
    Copy,
    RotateCcw,
  } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Button } from "$lib/components/ui/button";
  import { ContextMenu } from "$lib/components/ui/context-menu";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";
  import { cn } from "$lib/utils";
  import { toolIcon } from "./toolCatalog";
  import type { ToolStatusDto } from "$lib/stores/toolGovernance";

  interface Props {
    tool: ToolStatusDto;
    title: string;
    description: string;
    warning: string | null;
    canConfigure: boolean;
    /** List-level reload in flight: disables the toggle. */
    busy: boolean;
    onToggle: (enabled: boolean) => Promise<void> | void;
    onConfigure: () => void;
    /** Opens the read-only contract panel (descriptor + input schema). */
    onInspect: () => void;
    onReset: () => void;
    onCopyId: () => void;
  }

  let {
    tool,
    title,
    description,
    warning,
    canConfigure,
    busy,
    onToggle,
    onConfigure,
    onInspect,
    onReset,
    onCopyId,
  }: Props = $props();

  let toggling = $state(false);

  const Icon = $derived(toolIcon(tool.name));

  type Status = "on" | "off" | "attention" | "toggling";
  const status = $derived<Status>(
    toggling
      ? "toggling"
      : warning
        ? "attention"
        : tool.enabled
          ? "on"
          : "off",
  );

  const STATUS_KEY: Record<Status, string> = {
    on: "settings.tools_page.status_on",
    off: "settings.tools_page.status_off",
    attention: "settings.tools_page.status_attention",
    toggling: "settings.tools_page.status_toggling",
  };
  const STATUS_PILL: Record<Status, string> = {
    on: "bg-success/10 text-success-a11y",
    off: "bg-muted text-muted-foreground",
    attention: "bg-warning/15 text-warning-a11y",
    toggling: "bg-muted text-muted-foreground",
  };
  const STATUS_DOT: Record<Status, string> = {
    on: "bg-success",
    off: "bg-muted-foreground/60",
    attention: "bg-warning",
    toggling: "bg-muted-foreground/60",
  };

  const iconToneClass = $derived(
    status === "attention"
      ? "bg-warning/15 text-warning-a11y"
      : tool.enabled
        ? "bg-primary/10 text-primary"
        : "bg-muted text-muted-foreground",
  );

  async function handleChange(checked: boolean): Promise<void> {
    if (toggling) return;
    toggling = true;
    try {
      await onToggle(checked);
    } finally {
      toggling = false;
    }
  }

  const menuItems = $derived<ActionMenuItem[]>(
    [
      {
        id: "toggle",
        label: tool.enabled
          ? $t("settings.tools_page.ctx_disable")
          : $t("settings.tools_page.ctx_enable"),
        icon: Power,
        onclick: () => void handleChange(!tool.enabled),
        testid: `tool-ctx-toggle-${tool.name}`,
      },
      ...(canConfigure
        ? [
            {
              id: "configure",
              label: $t("settings.tools_page.ctx_configure"),
              icon: Settings2,
              onclick: onConfigure,
              testid: `tool-ctx-configure-${tool.name}`,
            },
          ]
        : []),
      {
        id: "inspect",
        label: $t("settings.tools_page.ctx_inspect"),
        icon: Braces,
        onclick: onInspect,
        testid: `tool-ctx-inspect-${tool.name}`,
      },
      {
        id: "copy",
        label: $t("settings.tools_page.ctx_copy_id"),
        icon: Copy,
        onclick: onCopyId,
        testid: `tool-ctx-copy-${tool.name}`,
      },
      ...(canConfigure
        ? [
            {
              id: "reset",
              label: $t("settings.tools_page.ctx_reset"),
              icon: RotateCcw,
              variant: "destructive" as const,
              onclick: onReset,
              testid: `tool-ctx-reset-${tool.name}`,
            },
          ]
        : []),
    ],
  );
</script>

<ContextMenu items={menuItems} data-testid="tool-context-menu-{tool.name}">
  <div
    data-tool-row
    data-testid="tool-card-{tool.name}"
    aria-label={title}
    class={cn(
      "flex items-center gap-3 rounded-lg border bg-card px-3.5 py-3 outline-none transition duration-150",
      "hover:shadow-sm focus-visible:border-primary/55 focus-visible:ring-2 focus-visible:ring-primary/15 focus-within:border-primary/55",
      status === "attention" ? "border-warning/45" : "border-border",
      !tool.enabled && "bg-card/60",
    )}
  >
    <span class={cn("flex size-8 shrink-0 items-center justify-center rounded-md", iconToneClass)}>
      <Icon size={17} strokeWidth={1.75} aria-hidden="true" />
    </span>

    <div class="min-w-0 flex-1">
      <div class="flex flex-wrap items-center gap-2">
        <span class={cn("text-body-sm font-medium", tool.enabled ? "text-foreground" : "text-muted-foreground")}>
          {title}
        </span>
        <span class="font-mono text-overline tracking-tight text-muted-foreground/80">{tool.name}</span>
        <span
          class={cn(
            "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-overline font-semibold",
            STATUS_PILL[status],
          )}
        >
          <span class={cn("size-1 rounded-full", STATUS_DOT[status])}></span>
          {$t(STATUS_KEY[status])}
        </span>
      </div>
      {#if description}
        <p class={cn("mt-0.5 max-w-prose text-caption", tool.enabled ? "text-muted-foreground" : "text-muted-foreground/80")}>
          {description}
        </p>
      {/if}
      {#if warning}
        <p class="mt-1 inline-flex items-center gap-1 text-caption text-warning-a11y">
          <AlertTriangle size={12} strokeWidth={2} aria-hidden="true" />
          {warning}
        </p>
      {/if}
    </div>

    <div class="flex shrink-0 items-center gap-1.5">
      <Button
        variant="ghost"
        size="icon-sm"
        onclick={onInspect}
        title={$t("settings.tools_page.inspect")}
        aria-label={$t("settings.tools_page.inspect_aria", { values: { name: title } })}
        data-testid="tool-inspect-{tool.name}"
      >
        <Braces size={14} strokeWidth={1.9} aria-hidden="true" />
      </Button>
      {#if canConfigure}
        <Button
          variant="ghost"
          size="sm"
          onclick={onConfigure}
          data-testid="tool-configure-{tool.name}"
        >
          {$t("settings.tools_page.configure")}
          <ChevronRight size={14} strokeWidth={2} class="ml-0.5" aria-hidden="true" />
        </Button>
      {/if}
      <Toggle
        checked={tool.enabled}
        loading={toggling || busy}
        onchange={handleChange}
        aria-label={title}
        data-testid="tool-toggle-{tool.name}"
      />
    </div>
  </div>
</ContextMenu>
