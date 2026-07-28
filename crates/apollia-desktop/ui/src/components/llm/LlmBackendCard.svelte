<script lang="ts">
  /**
   * LlmBackendCard - one backend on the top-level "AI Models" surface.
   *
   * Operator mode shows a humanized title, a single cost badge, a plain-language
   * status line, and (on failure) a humanized reason. Builder mode shows the raw
   * `backend.name`, type/status/default badges, the model, the ping result, and
   * an inline action cluster. Both modes share a right-click ContextMenu
   * mirroring the settings CRUD actions. The default backend is the focal card
   * and carries expressive depth (primary ring + tinted shadow).
   */
  import { t } from "svelte-i18n";
  import { Brain, Cloud, AlertTriangle, Star, Pencil, Trash2, Plug } from "lucide-svelte";
  import type { LlmBackendConfig } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { humanize } from "$lib/errors/humanize";
  import { pingLlmBackend } from "$lib/ipc/llm";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { EntityCard, StatusDot } from "$lib/components/operator";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";

  interface Props {
    backend: LlmBackendConfig;
    onSetDefault: (backend: LlmBackendConfig) => void;
    onEdit: (backend: LlmBackendConfig) => void;
    onRemove: (backend: LlmBackendConfig) => void;
  }

  let { backend, onSetDefault, onEdit, onRemove }: Props = $props();

  const FEEDBACK_MS = 5_000;
  let pinging = $state(false);
  let pingLatency = $state<number | null>(null);
  let pingError = $state<string | null>(null);
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;

  const isBuilder = $derived($uiMode === "builder");
  const isLocal = $derived(backend.provider === "llama-cpp");
  const inError = $derived(!backend.enabled || !!backend.last_ping_error);
  const focal = $derived(backend.is_default);

  type Tone = "primary" | "info" | "destructive";
  const tone = $derived<Tone>(inError ? "destructive" : isLocal ? "primary" : "info");
  const IconComp = $derived(inError ? AlertTriangle : isLocal ? Brain : Cloud);

  function prettify(model: string): string {
    return model.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }

  const humanizedTitle = $derived.by(() => {
    const model = prettify(backend.model || backend.name);
    const status = inError ? $t("llm.status_error") : $t("llm.running");
    return isLocal
      ? `${$t("llm.local_ai")} (${model}) · ${status}`
      : `${model} · ${status}`;
  });

  const operatorReason = $derived.by(() => {
    if (!inError || !backend.last_ping_error) return null;
    const h = humanize(backend.last_ping_error, (k) => $t(k));
    return `${h.friendly_message} ${h.suggested_action}`;
  });

  async function handlePing() {
    if (pinging || !backend.enabled) return;
    pinging = true;
    pingLatency = null;
    pingError = null;
    if (feedbackTimer) clearTimeout(feedbackTimer);
    try {
      const result = await pingLlmBackend(backend.name);
      if (result.available && result.latency_ms !== null) {
        pingLatency = result.latency_ms;
      } else {
        pingError = result.error ?? $t("llm.ping_failed");
      }
    } catch (err: unknown) {
      pingError = err instanceof Error ? err.message : String(err);
    } finally {
      pinging = false;
      feedbackTimer = setTimeout(() => {
        pingLatency = null;
        pingError = null;
      }, FEEDBACK_MS);
    }
  }

  const menuItems = $derived<ActionMenuItem[]>([
    {
      id: "set-default",
      label: $t("settings.llm_set_default"),
      icon: Star,
      disabled: backend.is_default,
      onclick: () => onSetDefault(backend),
      testid: "set-default",
    },
    {
      id: "edit",
      label: $t("settings.llm.edit"),
      icon: Pencil,
      onclick: () => onEdit(backend),
      testid: "edit-backend",
    },
    {
      id: "test",
      label: $t("settings.llm.test"),
      icon: Plug,
      disabled: !backend.enabled || pinging,
      onclick: handlePing,
    },
    {
      id: "remove",
      label: $t("common.delete"),
      icon: Trash2,
      variant: "destructive",
      onclick: () => onRemove(backend),
      testid: "delete-backend",
    },
  ]);
</script>

{#snippet cardIcon()}
  <IconComp size={17} strokeWidth={2} aria-hidden="true" />
{/snippet}

{#snippet builderBadges()}
  {#if backend.is_default}
    <Badge variant="primary" outline size="sm">{$t("common.default_badge")}</Badge>
  {/if}
  <Badge variant={isLocal ? "info" : "neutral"} size="sm" data-testid="llm-backend-type-badge">
    {isLocal ? "EMBEDDED" : "API"}
  </Badge>
  <Badge variant={inError ? "danger" : "success"} size="sm" data-testid="llm-backend-badge">
    {inError ? $t("common.status.error") : $t("common.status.ready")}
  </Badge>
{/snippet}

{#snippet operatorBadges()}
  <Badge variant={isLocal ? "success" : "neutral"} outline size="sm" data-testid="llm-backend-badge">
    {isLocal ? $t("llm.free_local") : $t("llm.pay_per_use")}
  </Badge>
{/snippet}

{#snippet builderBody()}
  <p class="truncate text-body-xs text-muted-foreground" title={backend.model}>
    {$t("llm.model")}: <span class="font-mono text-foreground/80">{backend.model}</span>
  </p>
  {#if pingLatency !== null}
    <p class="text-body-xs text-success" data-testid="llm-ping-result">
      {$t("llm.ping_ok", { values: { latency: pingLatency } })}
    </p>
  {:else if pingError}
    <p class="truncate text-body-xs text-destructive" data-testid="llm-ping-result" title={pingError}>
      {pingError}
    </p>
  {/if}
{/snippet}

{#snippet operatorBody()}
  <div class="flex items-center gap-2 text-body-xs text-muted-foreground">
    <StatusDot color={inError ? "hsl(var(--destructive))" : "hsl(var(--success))"} size={8} />
    <span>{inError ? $t("llm.status_error") : $t("llm.running")}</span>
  </div>
  {#if operatorReason}
    <p class="text-body-xs text-destructive">{operatorReason}</p>
  {/if}
{/snippet}

{#snippet builderActions()}
  {#if !backend.is_default}
    <Button variant="ghost" size="sm" class="text-primary" onclick={() => onSetDefault(backend)} data-testid="set-default">
      {$t("settings.llm_set_default")}
    </Button>
  {/if}
  <Button variant="ghost" size="icon-sm" onclick={handlePing} disabled={!backend.enabled || pinging} title={$t("settings.llm.test")} aria-label={$t("settings.llm.test")} data-testid="llm-backend-ping-btn">
    <Plug size={15} class={pinging ? "animate-pulse" : ""} />
  </Button>
  <Button variant="ghost" size="icon-sm" onclick={() => onEdit(backend)} title={$t("settings.llm.edit")} aria-label={$t("settings.llm.edit")} data-testid="edit-backend">
    <Pencil size={15} />
  </Button>
  <Button variant="ghost" size="icon-sm" class="hover:bg-destructive/10 hover:text-destructive" onclick={() => onRemove(backend)} title={$t("common.delete")} aria-label={$t("common.delete")} data-testid="delete-backend">
    <Trash2 size={15} />
  </Button>
{/snippet}

<ContextMenu items={menuItems} data-testid="llm-backend-menu">
  <EntityCard
    accent={tone}
    iconTone={tone}
    icon={cardIcon}
    title={isBuilder ? backend.name : humanizedTitle}
    subtitle={isBuilder ? `${backend.provider} · ${backend.model}` : undefined}
    badges={isBuilder ? builderBadges : operatorBadges}
    body={isBuilder ? builderBody : operatorBody}
    actions={isBuilder ? builderActions : undefined}
    class={`outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 ${focal ? "ring-1 ring-primary/40 shadow-primary-lg" : ""}`}
    data-testid="llm-backend-card"
  />
</ContextMenu>
