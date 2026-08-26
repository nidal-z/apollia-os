<script lang="ts">
  /**
   * Field set of an automation definition, bound to a single `AutomationFormModel`.
   *
   * Shared surface for the create and the edit paths: the dialogs own loading,
   * submitting and error reporting, this component owns the fields. Anything
   * added here shows up on both paths, which is the point.
   */
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import {
    Calendar,
    Timer,
    CalendarCheck,
    FolderOpen,
    Webhook,
    PlusCircle,
    Pencil,
    Trash2,
    ChevronDown,
  } from "lucide-svelte";
  import type { AgentListItem } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Toggle } from "$lib/components/ui/toggle";
  import { DatePicker, TimePicker } from "$lib/components/ui/date-picker";
  import { FormField } from "$lib/components/ui/form-field";
  import CronBuilder from "../triggers/CronBuilder.svelte";
  import IntervalPicker from "../triggers/IntervalPicker.svelte";
  import {
    generateWebhookSecret,
    type AutomationFormErrors,
    type AutomationFormModel,
    type AutomationSourceType,
  } from "./automationDefinition";

  interface Props {
    /** Bound form model, mutated in place by the fields. */
    model: AutomationFormModel;
    /** `edit` locks the identifier and treats a stored webhook secret as set. */
    mode: "create" | "edit";
    agents: AgentListItem[];
    /** Translation keys of the active errors, all `null` before first submit. */
    errors: AutomationFormErrors;
    /** Prefix for every `data-testid` of the field set. */
    testidPrefix: string;
  }

  let { model = $bindable(), mode, agents, errors, testidPrefix }: Props = $props();

  const TRIGGER_TYPE_CONFIGS: {
    value: AutomationSourceType;
    labelKey: string;
    descKey: string;
    icon: typeof Calendar;
  }[] = [
    { value: "cron", labelKey: "triggers.type_cron_label", descKey: "triggers.type_cron_desc", icon: Calendar },
    { value: "interval", labelKey: "triggers.type_interval_label", descKey: "triggers.type_interval_desc", icon: Timer },
    { value: "oneshot", labelKey: "triggers.type_oneshot_label", descKey: "triggers.type_oneshot_desc", icon: CalendarCheck },
    { value: "file_watch", labelKey: "triggers.type_file_watch_label", descKey: "triggers.type_file_watch_desc", icon: FolderOpen },
    { value: "webhook", labelKey: "triggers.type_webhook_label", descKey: "triggers.type_webhook_desc", icon: Webhook },
  ];

  let advancedOpen = $state(false);

  const sourceErrorText = $derived(errors.source ? $t(errors.source) : "");
  const targetErrorText = $derived(errors.target ? $t(errors.target) : "");
  const idErrorText = $derived(errors.id ? $t(errors.id) : "");

  function rotateSecret() {
    model.webhookSecret = generateWebhookSecret();
  }
</script>

<div class="space-y-5">
  <!-- Target: agent -->
  <div>
    <p class="mb-1.5 block text-caption font-medium text-muted-foreground">{$t("triggers.field_target")}</p>
    <Select
      bind:value={model.agent}
      aria-label={$t("triggers.field_target_agent")}
      data-testid="{testidPrefix}-agent"
    >
      <option value="" disabled>- {$t("triggers.field_target_agent")} -</option>
      {#each agents as agent}
        <option value={agent.name}>{agent.name}</option>
      {/each}
    </Select>
    {#if targetErrorText}
      <p class="mt-0.5 text-xs text-destructive" data-testid="{testidPrefix}-target-error">{targetErrorText}</p>
    {/if}
  </div>

  <!-- Source type -->
  <div>
    <p class="mb-2 block text-caption font-medium text-muted-foreground">{$t("triggers.field_source_type")}</p>
    <div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label={$t("triggers.field_source_type")}>
      {#each TRIGGER_TYPE_CONFIGS as typeConfig}
        <button
          type="button"
          role="radio"
          aria-checked={model.sourceType === typeConfig.value}
          class="flex items-start gap-2.5 rounded-lg border p-3 text-left transition-colors
            {model.sourceType === typeConfig.value
              ? 'border-primary bg-primary/5 ring-1 ring-primary'
              : 'border-border hover:border-primary/40 hover:bg-muted/30'}
            {typeConfig.value === 'webhook' ? 'col-span-2' : ''}"
          onclick={() => { model.sourceType = typeConfig.value; }}
          data-testid="{testidPrefix}-type-{typeConfig.value}"
        >
          <typeConfig.icon
            size={16}
            class="mt-0.5 shrink-0 {model.sourceType === typeConfig.value ? 'text-primary' : 'text-muted-foreground'}"
          />
          <div class="min-w-0">
            <p class="text-body-sm font-medium leading-tight">{$t(typeConfig.labelKey)}</p>
            <p class="mt-0.5 text-caption leading-snug text-muted-foreground">{$t(typeConfig.descKey)}</p>
          </div>
        </button>
      {/each}
    </div>
  </div>

  <!-- Source fields -->
  {#if model.sourceType === "cron"}
    <div>
      <p class="mb-2 block text-caption font-medium text-muted-foreground">{$t("triggers.field_schedule")}</p>
      <div data-testid="{testidPrefix}-cron">
        <CronBuilder
          value={model.cronSchedule}
          onchange={(expr) => { model.cronSchedule = expr; }}
        />
      </div>
      {#if sourceErrorText}
        <p class="mt-1 text-xs text-destructive" data-testid="{testidPrefix}-source-error">{sourceErrorText}</p>
      {/if}
    </div>

  {:else if model.sourceType === "interval"}
    <div>
      <p class="mb-2 block text-caption font-medium text-muted-foreground">{$t("triggers.field_every")}</p>
      <div data-testid="{testidPrefix}-interval">
        <IntervalPicker
          value={model.intervalEvery}
          onchange={(val) => { model.intervalEvery = val; }}
        />
      </div>
      {#if sourceErrorText}
        <p class="mt-1 text-xs text-destructive" data-testid="{testidPrefix}-source-error">{sourceErrorText}</p>
      {/if}
    </div>

  {:else if model.sourceType === "oneshot"}
    <FormField
      id="{testidPrefix}-fire-at"
      label={$t("triggers.field_fire_at")}
      labelClass="text-caption mb-0.5"
      error={sourceErrorText || undefined}
    >
      <div class="grid grid-cols-2 gap-2">
        <DatePicker
          id="{testidPrefix}-fire-at"
          class={sourceErrorText ? "border-destructive" : ""}
          bind:value={model.oneshotDate}
          data-testid="{testidPrefix}-fire-at-date"
        />
        <TimePicker
          class={sourceErrorText ? "border-destructive" : ""}
          bind:value={model.oneshotTime}
          data-testid="{testidPrefix}-fire-at-time"
        />
      </div>
    </FormField>

  {:else if model.sourceType === "file_watch"}
    <div class="space-y-3">
      <FormField
        id="{testidPrefix}-watch-path"
        label={$t("triggers.field_path")}
        labelClass="text-caption mb-0.5"
        hint={$t("triggers.field_path_help")}
      >
        <Input
          id="{testidPrefix}-watch-path"
          class={sourceErrorText && !model.filePath.trim() ? "border-destructive" : ""}
          placeholder={$t("triggers.field_path_placeholder")}
          bind:value={model.filePath}
          data-testid="{testidPrefix}-filepath"
        />
      </FormField>
      <div>
        <p class="mb-2 block text-caption font-medium text-muted-foreground">{$t("triggers.field_events")}</p>
        <div class="flex gap-2">
          <button
            type="button"
            class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
              {model.fileCreate ? 'border-primary bg-primary/10 text-primary' : 'border-border text-muted-foreground hover:border-primary/40'}"
            onclick={() => { model.fileCreate = !model.fileCreate; }}
            data-testid="{testidPrefix}-event-create"
          >
            <PlusCircle size={11} />
            {$t("triggers.field_events_create")}
          </button>
          <button
            type="button"
            class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
              {model.fileModify ? 'border-primary bg-primary/10 text-primary' : 'border-border text-muted-foreground hover:border-primary/40'}"
            onclick={() => { model.fileModify = !model.fileModify; }}
            data-testid="{testidPrefix}-event-modify"
          >
            <Pencil size={11} />
            {$t("triggers.field_events_modify")}
          </button>
          <button
            type="button"
            class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
              {model.fileDelete ? 'border-destructive/80 bg-destructive/10 text-destructive' : 'border-border text-muted-foreground hover:border-destructive/40'}"
            onclick={() => { model.fileDelete = !model.fileDelete; }}
            data-testid="{testidPrefix}-event-delete"
          >
            <Trash2 size={11} />
            {$t("triggers.field_events_delete")}
          </button>
        </div>
        {#if sourceErrorText && model.filePath.trim()}
          <p class="mt-1 text-xs text-destructive" data-testid="{testidPrefix}-source-error">{sourceErrorText}</p>
        {/if}
      </div>
      <div>
        <label class="flex items-start gap-2 cursor-pointer">
          <Toggle bind:checked={model.fileRecursive} data-testid="{testidPrefix}-recursive" />
          <span class="flex-1">
            <span class="block text-body-xs font-medium text-foreground">{$t("triggers.field_recursive_label")}</span>
            <span class="mt-0.5 block text-caption text-muted-foreground">{$t("triggers.field_recursive_hint")}</span>
          </span>
        </label>
      </div>
    </div>

  {:else if model.sourceType === "webhook"}
    <div class="space-y-3">
      <div class="rounded-md border border-border bg-muted/30 px-3.5 py-2.5 text-body-xs text-muted-foreground leading-relaxed">
        {$t("triggers.webhook_explain")}
      </div>
      <FormField
        id="{testidPrefix}-secret"
        label={$t("triggers.field_secret")}
        labelClass="text-caption mb-0.5"
        error={sourceErrorText || undefined}
        hint={mode === "edit" && model.hasStoredWebhookSecret
          ? $t("triggers.field_secret_rotate_hint")
          : undefined}
      >
        <div class="flex gap-2">
          <Input
            id="{testidPrefix}-secret"
            class="flex-1 font-mono text-xs {sourceErrorText ? 'border-destructive' : ''}"
            placeholder={mode === "edit" && model.hasStoredWebhookSecret
              ? $t("triggers.field_secret_kept")
              : $t("triggers.field_secret_placeholder")}
            bind:value={model.webhookSecret}
            data-testid="{testidPrefix}-secret"
          />
          <Button size="sm" variant="outline" onclick={rotateSecret} data-testid="{testidPrefix}-secret-generate">
            {mode === "edit" && model.hasStoredWebhookSecret
              ? $t("triggers.field_secret_rotate")
              : $t("triggers.field_secret_generate")}
          </Button>
        </div>
      </FormField>
      {#if mode === "edit" && model.webhookSecret.trim()}
        <p class="text-caption text-warning-a11y" data-testid="{testidPrefix}-secret-warning">
          {$t("triggers.field_secret_rotate_warning")}
        </p>
      {/if}
    </div>
  {/if}

  <!-- Enabled -->
  <label class="flex items-center gap-2 text-sm">
    <Toggle bind:checked={model.enabled} data-testid="{testidPrefix}-enabled" />
    {$t("triggers.field_enabled")}
  </label>

  <!-- Advanced -->
  <div class="rounded-md border border-border">
    <button
      type="button"
      class="flex w-full items-center justify-between px-3 py-2 text-caption font-medium text-muted-foreground transition-colors hover:bg-muted/30"
      onclick={() => { advancedOpen = !advancedOpen; }}
      data-testid="{testidPrefix}-advanced-toggle"
    >
      {$t("triggers.advanced_section")}
      <ChevronDown
        size={14}
        class="transition-transform duration-base {advancedOpen ? 'rotate-180' : ''}"
      />
    </button>
    {#if advancedOpen}
      <div
        class="space-y-4 border-t border-border px-3 pb-3 pt-3"
        transition:slide={{ duration: 180 }}
      >
        <!-- Identifier -->
        {#if mode === "edit"}
          <div>
            <p class="mb-0.5 text-caption font-medium text-muted-foreground">{$t("triggers.field_id")}</p>
            <p class="font-mono text-body-xs text-foreground" data-testid="{testidPrefix}-id-readonly">{model.id}</p>
            <p class="mt-0.5 text-caption text-muted-foreground/70">{$t("triggers.field_id_immutable")}</p>
          </div>
        {:else}
          <FormField
            id="{testidPrefix}-id"
            label={$t("triggers.field_id")}
            labelClass="text-caption font-normal mb-0"
            error={idErrorText || undefined}
            hint={$t("triggers.field_id_help")}
          >
            <Input
              id="{testidPrefix}-id"
              class={idErrorText ? "border-destructive" : ""}
              placeholder={$t("triggers.field_id_placeholder")}
              bind:value={model.id}
              data-testid="{testidPrefix}-id"
            />
          </FormField>
        {/if}

        <!-- On busy -->
        <FormField
          id="{testidPrefix}-on-busy"
          label={$t("triggers.field_on_busy")}
          labelClass="text-caption font-normal mb-0"
        >
          <Select
            id="{testidPrefix}-on-busy"
            bind:value={model.onBusy}
            data-testid="{testidPrefix}-on-busy"
          >
            <option value="queue">{$t("triggers.field_on_busy_queue")}</option>
            <option value="drop">{$t("triggers.field_on_busy_drop")}</option>
          </Select>
        </FormField>

        <!-- Input template -->
        <FormField
          id="{testidPrefix}-input-template"
          label={$t("triggers.field_input_template")}
          labelClass="text-caption font-normal mb-0"
          optional
        >
          <Textarea
            id="{testidPrefix}-input-template"
            rows={2}
            placeholder={$t("triggers.field_input_template_placeholder")}
            bind:value={model.inputTemplate}
            data-testid="{testidPrefix}-input-template"
          />
        </FormField>
      </div>
    {/if}
  </div>
</div>
