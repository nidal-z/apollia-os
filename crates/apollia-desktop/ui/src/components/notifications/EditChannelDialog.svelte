<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { NotificationChannelView, UpdateChannelRequest } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Select } from "$lib/components/ui/select";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Textarea } from "$lib/components/ui/textarea";
  import { FormField } from "$lib/components/ui/form-field";
  import { eventLabelKey, eventDescriptionKey } from "$lib/notifications/event-labels";
  import {
    THROTTLE_PRESETS,
    MAX_MIN_INTERVAL_SECONDS,
    isPreset,
  } from "$lib/notifications/throttle-options";

  interface Props {
    open: boolean;
    channel: NotificationChannelView | null;
    globalEvents: string[];
    onclose: () => void;
    onupdated: (id: string) => void;
  }

  let { open, channel, globalEvents, onclose, onupdated }: Props = $props();

  type ChannelType = "desktop" | "webhook";

  const LABEL_MAX_LEN = 80;

  let label = $state("");
  let channelType = $state<ChannelType>("desktop");
  let enabled = $state(true);
  let webhookUrl = $state("");
  let headersText = $state("");
  let selectedEvents = $state<Set<string>>(new Set());
  let throttlePreset = $state<string>("0");
  let throttleCustom = $state("0"); // string for Input binding
  let originalThrottle = $state(0);

  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let touched = $state(false);

  const labelError = $derived.by(() => {
    if (!touched) return null;
    if (label.length > LABEL_MAX_LEN) return $t("notifications.field_label_too_long");
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

  const throttleValue = $derived(
    throttlePreset === "custom"
      ? Math.max(0, Math.floor(Number.parseFloat(throttleCustom) || 0))
      : parseInt(throttlePreset, 10) || 0,
  );

  const throttleError = $derived.by(() => {
    if (!touched) return null;
    if (throttleValue > MAX_MIN_INTERVAL_SECONDS) return $t("notifications.field_throttle_too_large");
    return null;
  });

  const isValid = $derived(
    label.length <= LABEL_MAX_LEN &&
    (channelType !== "webhook" || !!webhookUrl.trim()) &&
    !headersError &&
    !throttleError
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

      const trimmedLabel = label.trim();
      const originalLabel = channel.label ?? "";
      const request: UpdateChannelRequest = {
        channel_type: channelType,
        enabled,
        config,
      };
      // Only include `label` when changed — distinguishes "keep" from "clear".
      if (trimmedLabel !== originalLabel.trim()) {
        request.label = trimmedLabel || null;
      }
      if (selectedEvents.size > 0) {
        request.events = [...selectedEvents];
      }
      if (throttleValue !== originalThrottle) {
        request.min_interval_seconds = throttleValue;
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
    label = channel.label ?? "";
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
    originalThrottle = channel.min_interval_seconds ?? 0;
    if (isPreset(originalThrottle)) {
      throttlePreset = String(originalThrottle);
      throttleCustom = "0";
    } else {
      throttlePreset = "custom";
      throttleCustom = String(originalThrottle);
    }
    submitting = false;
    submitError = null;
    touched = false;
  }

  $effect(() => {
    if (open && channel) {
      populateForm();
    }
  });
</script>

{#if channel}
  <Dialog
    open={open && !!channel}
    {onclose}
    size="sm"
    title={$t("notifications.edit_channel")}
    data-testid="edit-channel-dialog"
  >
    <div class="space-y-4">
      <!-- Label (free-form display name) -->
      <FormField
        id="edit-channel-label"
        label={$t("notifications.field_label")}
        labelClass="text-[11px] font-normal mb-0"
        error={labelError || undefined}
        hint={$t("notifications.field_label_help")}
      >
        <Input
          id="edit-channel-label"
          class={labelError ? 'border-destructive' : ''}
          placeholder={$t("notifications.field_label_placeholder")}
          bind:value={label}
          maxlength={LABEL_MAX_LEN}
          data-testid="edit-channel-label-input"
        />
      </FormField>

      <!-- ID (readonly, slug fixed at creation time) -->
      <FormField
        id="edit-channel-id"
        label={$t("notifications.field_id")}
        labelClass="text-[11px] font-normal mb-0"
      >
        <Input
          id="edit-channel-id"
          class="bg-muted font-mono"
          value={channel.id}
          readonly
          data-testid="edit-channel-id-input"
        />
      </FormField>

      <!-- Type -->
      <FormField
        id="edit-channel-type"
        label={$t("notifications.field_type")}
        labelClass="text-[11px] font-normal mb-0"
      >
        <Select
          id="edit-channel-type"
          bind:value={channelType}
          data-testid="edit-channel-type-select"
        >
          <option value="desktop">{$t("notifications.field_type_desktop")}</option>
          <option value="webhook">{$t("notifications.field_type_webhook")}</option>
        </Select>
      </FormField>

      <!-- Dynamic webhook fields -->
      {#if channelType === "webhook"}
        <FormField
          id="edit-channel-url"
          label={$t("notifications.field_url")}
          labelClass="text-[11px] font-normal mb-0"
          error={urlError || undefined}
        >
          <Input
            id="edit-channel-url"
            type="url"
            class={urlError ? 'border-destructive' : ''}
            placeholder={$t("notifications.field_url_placeholder")}
            bind:value={webhookUrl}
            data-testid="edit-channel-url-input"
          />
        </FormField>

        <FormField
          id="edit-channel-headers"
          label={$t("notifications.field_headers")}
          labelClass="text-[11px] font-normal mb-0"
          optional
          error={headersError || undefined}
        >
          <Textarea
            id="edit-channel-headers"
            class="font-mono text-sm {headersError ? 'border-destructive' : ''}"
            rows={2}
            placeholder={$t("notifications.field_headers_placeholder")}
            bind:value={headersText}
            data-testid="edit-channel-headers-textarea"
          />
        </FormField>
      {/if}

      <!-- Events per-channel -->
      {#if globalEvents.length > 0}
        <div>
          <p class="mb-1 block text-[11px] text-muted-foreground">{$t("notifications.field_events")}</p>
          <p class="mb-2 text-xs text-muted-foreground">{$t("notifications.field_events_hint")}</p>
          <div class="grid grid-cols-1 gap-x-4 gap-y-2 sm:grid-cols-2">
            {#each globalEvents as event}
              <label class="flex items-start gap-2 text-sm" data-testid="edit-channel-event-{event}">
                <Checkbox
                  class="mt-0.5"
                  checked={selectedEvents.has(event)}
                  onchange={() => toggleEvent(event)}
                />
                <span class="flex flex-1 flex-col gap-0.5 min-w-0">
                  <span class="font-medium leading-tight">{$t(eventLabelKey(event))}</span>
                  <span class="text-[11px] leading-snug text-muted-foreground">
                    {$t(eventDescriptionKey(event))}
                  </span>
                  <span class="text-[10px] font-mono text-muted-foreground/60 truncate">
                    {event}
                  </span>
                </span>
              </label>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Throttle -->
      <FormField
        id="edit-channel-throttle"
        label={$t("notifications.field_throttle")}
        labelClass="text-[11px] font-normal mb-0"
        hint={$t("notifications.field_throttle_help")}
      >
        <Select
          id="edit-channel-throttle"
          bind:value={throttlePreset}
          data-testid="edit-channel-throttle-select"
        >
          {#each THROTTLE_PRESETS as preset}
            <option value={String(preset.value)}>{$t(preset.key)}</option>
          {/each}
          <option value="custom">{$t("notifications.throttle_options.custom")}</option>
        </Select>
        {#if throttlePreset === "custom"}
          <FormField
            id="edit-channel-throttle-custom"
            label={$t("notifications.throttle_custom_label")}
            labelClass="text-[11px] font-normal mb-0"
            class="mt-2"
          >
            <Input
              id="edit-channel-throttle-custom"
              type="number"
              min={1}
              max={MAX_MIN_INTERVAL_SECONDS}
              bind:value={throttleCustom}
              data-testid="edit-channel-throttle-custom-input"
            />
          </FormField>
        {/if}
        {#if throttleError}
          <p class="mt-0.5 text-xs text-destructive" data-testid="edit-channel-throttle-error">{throttleError}</p>
        {/if}
      </FormField>

      <!-- Enabled toggle -->
      <label class="flex items-center gap-2 text-sm">
        <Checkbox bind:checked={enabled} data-testid="edit-channel-enabled-toggle" />
        {$t("notifications.field_enabled")}
      </label>

      <!-- Submit error -->
      {#if submitError}
        <p class="text-sm text-destructive" data-testid="edit-channel-error">{submitError}</p>
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
  </Dialog>
{/if}
