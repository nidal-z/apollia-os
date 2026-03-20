<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { NotificationChannelView, UpdateChannelRequest } from "$lib/types";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    open: boolean;
    channel: NotificationChannelView | null;
    globalEvents: string[];
    onclose: () => void;
    onupdated: (id: string) => void;
  }

  let { open, channel, globalEvents, onclose, onupdated }: Props = $props();

  type ChannelType = "desktop" | "webhook";

  let channelType = $state<ChannelType>("desktop");
  let enabled = $state(true);
  let webhookUrl = $state("");
  let headersText = $state("");
  let selectedEvents = $state<Set<string>>(new Set());

  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let touched = $state(false);

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
    if (!channel) return;
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

      const request: UpdateChannelRequest = {
        channel_type: channelType,
        enabled,
        config,
      };
      if (selectedEvents.size > 0) {
        request.events = [...selectedEvents];
      }

      await invoke<NotificationChannelView>("update_notification_channel", {
        id: channel.id,
        channel: request,
      });
      onupdated(channel.id);
      onclose();
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  function populateForm() {
    if (!channel) return;
    channelType = channel.channel_type as ChannelType;
    enabled = channel.enabled;
    if (channelType === "webhook") {
      webhookUrl = (channel.config.url as string) ?? "";
      const headers = channel.config.headers;
      headersText = headers ? JSON.stringify(headers, null, 2) : "";
    } else {
      webhookUrl = "";
      headersText = "";
    }
    selectedEvents = channel.events ? new Set(channel.events) : new Set();
    submitting = false;
    submitError = null;
    touched = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      onclose();
    }
  }

  $effect(() => {
    if (open && channel) {
      populateForm();
    }
  });
</script>

{#if open && channel}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={onclose}
    onkeydown={handleKeydown}
  >
    <div
      class="max-h-[85vh] w-[480px] overflow-y-auto rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeydown}
      data-testid="edit-channel-dialog"
    >
      <h3 class="mb-4 text-lg font-semibold">{$t("notifications.edit_channel")}</h3>

      <div class="space-y-4">
        <!-- ID (readonly) -->
        <div>
          <label class="mb-1 block text-sm font-medium" for="edit-channel-id">{$t("notifications.field_id")}</label>
          <input
            id="edit-channel-id"
            type="text"
            class="w-full rounded-md border bg-muted px-3 py-2 text-sm"
            value={channel.id}
            readonly
            data-testid="edit-channel-id-input"
          />
        </div>

        <!-- Type -->
        <div>
          <label class="mb-1 block text-sm font-medium" for="edit-channel-type">{$t("notifications.field_type")}</label>
          <select
            id="edit-channel-type"
            class="w-full rounded-md border bg-background px-3 py-2 text-sm"
            bind:value={channelType}
            data-testid="edit-channel-type-select"
          >
            <option value="desktop">{$t("notifications.field_type_desktop")}</option>
            <option value="webhook">{$t("notifications.field_type_webhook")}</option>
          </select>
        </div>

        <!-- Dynamic webhook fields -->
        {#if channelType === "webhook"}
          <div>
            <label class="mb-1 block text-sm font-medium" for="edit-channel-url">{$t("notifications.field_url")}</label>
            <input
              id="edit-channel-url"
              type="url"
              class="w-full rounded-md border bg-background px-3 py-2 text-sm {urlError ? 'border-[hsl(var(--destructive))]' : ''}"
              placeholder={$t("notifications.field_url_placeholder")}
              bind:value={webhookUrl}
              data-testid="edit-channel-url-input"
            />
            {#if urlError}
              <p class="mt-0.5 text-xs text-[hsl(var(--destructive))]" data-testid="edit-channel-url-error">{urlError}</p>
            {/if}
          </div>

          <div>
            <label class="mb-1 block text-sm font-medium" for="edit-channel-headers">
              {$t("notifications.field_headers")}
              <span class="font-normal text-muted-foreground">({$t("pipelines.input_json_optional")})</span>
            </label>
            <textarea
              id="edit-channel-headers"
              class="w-full rounded-md border bg-background px-3 py-2 font-mono text-sm {headersError ? 'border-[hsl(var(--destructive))]' : ''}"
              rows="2"
              placeholder={$t("notifications.field_headers_placeholder")}
              bind:value={headersText}
              data-testid="edit-channel-headers-textarea"
            ></textarea>
            {#if headersError}
              <p class="mt-0.5 text-xs text-[hsl(var(--destructive))]" data-testid="edit-channel-headers-error">{headersError}</p>
            {/if}
          </div>
        {/if}

        <!-- Events per-channel -->
        {#if globalEvents.length > 0}
          <div>
            <label class="mb-1 block text-sm font-medium">{$t("notifications.field_events")}</label>
            <p class="mb-2 text-xs text-muted-foreground">{$t("notifications.field_events_hint")}</p>
            <div class="grid grid-cols-2 gap-x-4 gap-y-1.5">
              {#each globalEvents as event}
                <label class="flex items-center gap-2 text-sm" data-testid="edit-channel-event-{event}">
                  <input
                    type="checkbox"
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
          <input type="checkbox" bind:checked={enabled} data-testid="edit-channel-enabled-toggle" />
          {$t("notifications.field_enabled")}
        </label>

        <!-- Submit error -->
        {#if submitError}
          <p class="text-sm text-[hsl(var(--destructive))]" data-testid="edit-channel-error">{submitError}</p>
        {/if}

        <!-- Actions -->
        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" onclick={onclose} data-testid="edit-channel-cancel-btn">
            {$t("common.cancel")}
          </Button>
          <Button
            size="sm"
            onclick={handleSubmit}
            disabled={submitting || (touched && !isValid)}
            data-testid="edit-channel-submit-btn"
          >
            {submitting ? $t("notifications.saving") : $t("common.save")}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}
