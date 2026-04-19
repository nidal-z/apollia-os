<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { CreateChannelRequest, NotificationChannelView } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Select } from "$lib/components/ui/select";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Textarea } from "$lib/components/ui/textarea";

  interface Props {
    open: boolean;
    globalEvents: string[];
    onclose: () => void;
    oncreated: (id: string) => void;
  }

  let { open, globalEvents, onclose, oncreated }: Props = $props();

  type ChannelType = "desktop" | "webhook";

  const CHANNEL_ID_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/;

  let channelId = $state("");
  let channelType = $state<ChannelType>("desktop");
  let enabled = $state(true);
  let webhookUrl = $state("");
  let headersText = $state("");
  let selectedEvents = $state<Set<string>>(new Set());

  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let touched = $state(false);

  const idError = $derived.by(() => {
    if (!touched) return null;
    if (!channelId.trim()) return $t("notifications.field_id_required");
    if (!CHANNEL_ID_PATTERN.test(channelId)) return $t("notifications.field_id_invalid");
    return null;
  });

  const urlError = $derived.by(() => {
    if (!touched) return null;
    if (channelType !== "webhook") return null;
    if (!webhookUrl.trim()) return $t("notifications.field_url_required");
    return null;
  });

  const headersError = $derived.by(() => {
    if (!touched) return null;
    if (channelType !== "webhook") return null;
    if (!headersText.trim()) return null;
    try {
      JSON.parse(headersText);
      return null;
    } catch {
      return $t("notifications.field_headers_invalid");
    }
  });

  const isValid = $derived(
    !!channelId.trim() &&
    CHANNEL_ID_PATTERN.test(channelId) &&
    (channelType !== "webhook" || !!webhookUrl.trim()) &&
    !headersError
  );

  function toggleEvent(event: string) {
    const next = new Set(selectedEvents);
    if (next.has(event)) {
      next.delete(event);
    } else {
      next.add(event);
    }
    selectedEvents = next;
  }

  async function handleSubmit(): Promise<void> {
    touched = true;
    if (!isValid) return;

    submitting = true;
    submitError = null;
    try {
      const config: Record<string, unknown> = {};
      if (channelType === "webhook") {
        config.url = webhookUrl.trim();
        if (headersText.trim()) {
          config.headers = JSON.parse(headersText);
        }
      }

      const request: CreateChannelRequest = {
        id: channelId.trim(),
        channel_type: channelType,
        enabled,
        config,
      };
      if (selectedEvents.size > 0) {
        request.events = [...selectedEvents];
      }

      await invoke<NotificationChannelView>("create_notification_channel", {
        channel: request,
      });
      oncreated(channelId.trim());
      onclose();
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  function resetForm() {
    channelId = "";
    channelType = "desktop";
    enabled = true;
    webhookUrl = "";
    headersText = "";
    selectedEvents = new Set();
    submitting = false;
    submitError = null;
    touched = false;
  }

  $effect(() => {
    if (open) {
      resetForm();
    }
  });
</script>

<Dialog
  {open}
  {onclose}
  size="sm"
  title={$t("notifications.create_channel")}
  data-testid="create-channel-dialog"
>
  <div class="space-y-4">
    <!-- ID -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="channel-id">{$t("notifications.field_id")}</label>
      <Input
        id="channel-id"
        class={idError ? 'border-destructive' : ''}
        placeholder={$t("notifications.field_id_placeholder")}
        bind:value={channelId}
        data-testid="channel-id-input"
      />
      <p class="mt-0.5 text-xs text-muted-foreground">{$t("notifications.field_id_help")}</p>
      {#if idError}
        <p class="mt-0.5 text-xs text-destructive" data-testid="channel-id-error">{idError}</p>
      {/if}
    </div>

    <!-- Type -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="channel-type">{$t("notifications.field_type")}</label>
      <Select
        id="channel-type"
        bind:value={channelType}
        data-testid="channel-type-select"
      >
        <option value="desktop">{$t("notifications.field_type_desktop")}</option>
        <option value="webhook">{$t("notifications.field_type_webhook")}</option>
      </Select>
    </div>

    <!-- Dynamic webhook fields -->
    {#if channelType === "webhook"}
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="channel-url">{$t("notifications.field_url")}</label>
        <Input
          id="channel-url"
          type="url"
          class={urlError ? 'border-destructive' : ''}
          placeholder={$t("notifications.field_url_placeholder")}
          bind:value={webhookUrl}
          data-testid="channel-url-input"
        />
        {#if urlError}
          <p class="mt-0.5 text-xs text-destructive" data-testid="channel-url-error">{urlError}</p>
        {/if}
      </div>

      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="channel-headers">
          {$t("notifications.field_headers")}
          <span class="font-normal text-muted-foreground">({$t("pipelines.input_json_optional")})</span>
        </label>
        <Textarea
          id="channel-headers"
          class="font-mono text-sm {headersError ? 'border-destructive' : ''}"
          rows={2}
          placeholder={$t("notifications.field_headers_placeholder")}
          bind:value={headersText}
          data-testid="channel-headers-textarea"
        />
        {#if headersError}
          <p class="mt-0.5 text-xs text-destructive" data-testid="channel-headers-error">{headersError}</p>
        {/if}
      </div>
    {/if}

    <!-- Events per-channel -->
    {#if globalEvents.length > 0}
      <div>
        <p class="mb-1 block text-[11px] text-muted-foreground" role="group" aria-labelledby="channel-events-caption">
          <span id="channel-events-caption">{$t("notifications.field_events")}</span>
        </p>
        <p class="mb-2 text-xs text-muted-foreground">{$t("notifications.field_events_hint")}</p>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-1.5">
          {#each globalEvents as event}
            <label class="flex items-center gap-2 text-sm" data-testid="channel-event-{event}">
              <Checkbox
                checked={selectedEvents.has(event)}
                onchange={() => toggleEvent(event)}
              />
              <span class="font-mono text-xs">{event}</span>
            </label>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Enabled toggle -->
    <label class="flex items-center gap-2 text-sm">
      <Checkbox bind:checked={enabled} data-testid="channel-enabled-toggle" />
      {$t("notifications.field_enabled")}
    </label>

    <!-- Submit error -->
    {#if submitError}
      <p class="text-sm text-destructive" data-testid="create-channel-error">{submitError}</p>
    {/if}

    <!-- Actions -->
    <div class="flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={onclose} data-testid="create-channel-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button
        size="sm"
        onclick={handleSubmit}
        disabled={submitting || (touched && !isValid)}
        data-testid="create-channel-submit-btn"
      >
        {submitting ? $t("notifications.creating") : $t("notifications.create_channel")}
      </Button>
    </div>
  </div>
</Dialog>
