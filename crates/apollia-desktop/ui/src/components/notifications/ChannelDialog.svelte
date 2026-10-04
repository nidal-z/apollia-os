<script lang="ts">
  /**
   * The channel dialog, to add a channel or edit one: name, type, the address
   * and headers of a web call, the events the channel receives, the maximum
   * frequency, and whether it is on. "Every event" heads the event list,
   * checked for a new channel; an event switched off in the notified events is
   * greyed and says so.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import type { NotificationChannelView } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Select } from "$lib/components/ui/select";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Textarea } from "$lib/components/ui/textarea";
  import { FormField } from "$lib/components/ui/form-field";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { NOTIFICATION_EVENT_TYPES, eventLabelKey } from "$lib/notifications/event-labels";
  import { THROTTLE_PRESETS, MAX_MIN_INTERVAL_SECONDS } from "$lib/notifications/throttle-options";
  import {
    LABEL_MAX_LEN,
    emptyForm,
    eventChecked,
    formErrors,
    formFromChannel,
    setAllEvents,
    toggleEvent,
    withLabel,
    type ChannelForm,
  } from "$lib/notifications/channelForm";
  import { addChannel, saveChannel } from "$lib/notifications/notificationsSection";

  interface Props {
    open: boolean;
    /** `null` adds a channel; a channel opens it for editing. */
    channel: NotificationChannelView | null;
    /** The notified events of the application. */
    notified: string[];
    onclose: () => void;
    onsaved: (name: string) => void;
  }

  let { open, channel, notified, onclose, onsaved }: Props = $props();

  /** Test id prefixes of each mode: the add dialog kept the historical ones. */
  const TESTIDS = {
    create: { dialog: "create-channel", field: "channel" },
    edit: { dialog: "edit-channel", field: "edit-channel" },
  } as const;

  const mode = $derived<"create" | "edit">(channel ? "edit" : "create");
  const p = $derived(TESTIDS[mode].field);
  const d = $derived(TESTIDS[mode].dialog);

  let form = $state<ChannelForm>(emptyForm());
  let idEdited = $state(false);
  let touched = $state(false);
  let submitting = $state(false);
  let submitError = $state<string | null>(null);

  const errors = $derived(touched ? formErrors(form, mode) : {});
  const valid = $derived(Object.keys(formErrors(form, mode)).length === 0);

  function err(key: string | undefined): string | undefined {
    return key ? $t(key) : undefined;
  }

  // Tracked on `open` and `channel` only: filling the form must not re-run
  // when the form itself changes.
  $effect(() => {
    if (!open) return;
    const source = channel;
    untrack(() => {
      form = source ? formFromChannel(source) : emptyForm();
      idEdited = false;
      touched = false;
      submitting = false;
      submitError = null;
    });
  });

  async function submit(): Promise<void> {
    touched = true;
    if (!valid) return;
    submitting = true;
    submitError = null;
    try {
      const saved = channel ? await saveChannel(channel, form) : await addChannel(form);
      onsaved(saved.label?.trim() ? saved.label : saved.id);
      onclose();
    } catch (e: unknown) {
      submitError = e instanceof Error ? e.message : String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<Dialog
  {open}
  {onclose}
  size="sm"
  title={$t(channel ? "notifications.edit_channel" : "settings.notifications.add_channel")}
  data-testid="{d}-dialog"
>
  <div class="space-y-4">
    <FormField
      id="{p}-label"
      label={$t("notifications.field_label")}
      labelClass="text-caption font-normal mb-0"
      error={err(errors.label)}
    >
      <Input
        id="{p}-label"
        placeholder={$t("notifications.field_label_placeholder")}
        value={form.label}
        oninput={(e: Event) => (form = withLabel(form, (e.target as HTMLInputElement).value, idEdited || !!channel))}
        maxlength={LABEL_MAX_LEN}
        data-testid="{p}-label-input"
      />
    </FormField>

    {#if !channel}
      <Disclosure testid="disc-technical-id">
        {#snippet summary()}{$t("notifications.field_id_advanced")}{/snippet}
        <Input
          id="channel-id"
          aria-label={$t("notifications.field_id")}
          class="font-mono"
          placeholder={$t("notifications.field_id_placeholder")}
          value={form.id}
          oninput={(e: Event) => {
            idEdited = true;
            form = { ...form, id: (e.target as HTMLInputElement).value };
          }}
          data-testid="channel-id-input"
        />
        <p class="mt-0.5 text-body-xs text-muted-foreground">{$t("notifications.field_id_help")}</p>
        {#if errors.id}
          <p class="mt-0.5 text-body-xs text-destructive" data-testid="channel-id-error">{$t(errors.id)}</p>
        {/if}
      </Disclosure>
    {/if}

    <FormField id="{p}-type" label={$t("notifications.field_type")} labelClass="text-caption font-normal mb-0">
      <Select id="{p}-type" bind:value={form.type} data-testid="{p}-type-select">
        <option value="desktop">{$t("settings.notifications.type_desktop")}</option>
        <option value="webhook">{$t("settings.notifications.type_webhook")}</option>
      </Select>
    </FormField>

    {#if form.type === "webhook"}
      <FormField
        id="{p}-url"
        label={$t("settings.notifications.field_address")}
        labelClass="text-caption font-normal mb-0"
        error={err(errors.url)}
      >
        <Input
          id="{p}-url"
          type="url"
          placeholder={$t("notifications.field_url_placeholder")}
          bind:value={form.url}
          data-testid="{p}-url-input"
        />
      </FormField>
      <Disclosure testid="{p}-headers-disclosure">
        {#snippet summary()}{$t("notifications.field_headers")}{/snippet}
        <Textarea
          id="{p}-headers"
          aria-label={$t("notifications.field_headers")}
          class="font-mono text-sm"
          rows={2}
          placeholder={$t("notifications.field_headers_placeholder")}
          bind:value={form.headers}
          data-testid="{p}-headers-textarea"
        />
        {#if errors.headers}
          <p class="mt-0.5 text-body-xs text-destructive">{$t(errors.headers)}</p>
        {/if}
      </Disclosure>
    {/if}

    <div role="group" aria-labelledby="{p}-events-caption">
      <p id="{p}-events-caption" class="mb-1 text-caption text-muted-foreground">
        {$t("settings.notifications.field_channel_events")}
      </p>
      <p class="mb-2 text-caption text-muted-foreground">{$t("settings.notifications.field_channel_events_hint")}</p>
      <label class="flex items-center gap-2 text-body-sm" data-testid="{p}-events-all">
        <Checkbox checked={form.allEvents} onchange={(on) => (form = setAllEvents(form, on))} />
        {$t("notifications.all_events")}
      </label>
      <ul class="mt-2 space-y-1.5 pl-6">
        {#each NOTIFICATION_EVENT_TYPES as event (event)}
          {@const off = !notified.includes(event)}
          <li>
            <label
              class="flex items-center gap-2 text-body-sm {off ? 'text-muted-foreground opacity-60' : ''}"
              data-testid="{p}-event-{event}"
            >
              <Checkbox
                checked={!off && eventChecked(form, event, notified)}
                disabled={off}
                onchange={() => (form = toggleEvent(form, event, notified))}
              />
              <span class="min-w-0 flex-1">{$t(eventLabelKey(event))}</span>
              {#if off}
                <span class="text-caption">{$t("settings.notifications.event_off")}</span>
              {/if}
            </label>
          </li>
        {/each}
      </ul>
    </div>

    <FormField
      id="{p}-throttle"
      label={$t("settings.notifications.field_frequency")}
      labelClass="text-caption font-normal mb-0"
      error={err(errors.throttle)}
      hint={$t("settings.notifications.field_frequency_hint")}
    >
      <Select id="{p}-throttle" bind:value={form.throttlePreset} data-testid="{p}-throttle-select">
        {#each THROTTLE_PRESETS as preset (preset.value)}
          <option value={String(preset.value)}>{$t(preset.key)}</option>
        {/each}
        <option value="custom">{$t("notifications.throttle_options.custom")}</option>
      </Select>
      {#if form.throttlePreset === "custom"}
        <FormField
          id="{p}-throttle-custom"
          label={$t("notifications.throttle_custom_label")}
          labelClass="text-caption font-normal mb-0"
          class="mt-2"
        >
          <Input
            id="{p}-throttle-custom"
            type="number"
            min={1}
            max={MAX_MIN_INTERVAL_SECONDS}
            bind:value={form.throttleCustom}
            data-testid="{p}-throttle-custom-input"
          />
        </FormField>
      {/if}
    </FormField>

    <label class="flex items-center gap-2 text-body-sm">
      <Checkbox bind:checked={form.enabled} data-testid="{p}-enabled-toggle" />
      {$t("notifications.field_enabled")}
    </label>

    {#if submitError}
      <p class="text-body-sm text-destructive" data-testid="{d}-error">{submitError}</p>
    {/if}

    <div class="flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={onclose} data-testid="{d}-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button
        size="sm"
        onclick={submit}
        disabled={submitting || (touched && !valid)}
        data-testid="{d}-submit-btn"
      >
        {#if submitting}
          {$t(channel ? "notifications.saving" : "notifications.creating")}
        {:else}
          {$t(channel ? "common.save" : "settings.notifications.add_channel")}
        {/if}
      </Button>
    </div>
  </div>
</Dialog>
