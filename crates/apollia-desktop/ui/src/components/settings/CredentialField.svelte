<script lang="ts">
  /**
   * CredentialField - a single write-only tool credential (masked value, edit /
   * reveal / save / delete, optional live test). Consumed by ToolConfigDrawer.
   * The prop API is stable; `placeholder` is an optional overridable addition.
   * Save and test failures are humanized (raw text behind a Disclosure).
   */
  import { t } from "svelte-i18n";
  import { Eye, EyeOff, Check, AlertTriangle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import CredentialErrorNotice from "./CredentialErrorNotice.svelte";
  import { humanize } from "$lib/errors/humanize";
  import {
    setCredential,
    deleteCredential,
    testCredential,
    type CredentialTestResultDto,
  } from "$lib/stores/toolGovernance";

  interface Props {
    toolName: string;
    keyName: string;
    label: string;
    configured: boolean;
    canTest?: boolean;
    /** Optional field-specific placeholder; falls back to the shared default. */
    placeholder?: string;
    "data-testid"?: string;
  }

  let {
    toolName,
    keyName,
    label,
    configured,
    canTest = false,
    placeholder,
    "data-testid": dataTestId,
  }: Props = $props();

  let editing = $state(false);
  let draft = $state("");
  let visible = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let validationKey = $state<string | null>(null);
  let rawSaveError = $state<string | null>(null);
  let testResult = $state<CredentialTestResultDto | null>(null);

  const saveError = $derived(rawSaveError ? humanize(rawSaveError, $t) : null);
  const testError = $derived(
    testResult && !testResult.ok
      ? humanize(testResult.error ?? "", $t)
      : null,
  );

  function startEdit(): void {
    editing = true;
    draft = "";
    visible = false;
    validationKey = null;
    rawSaveError = null;
    testResult = null;
  }

  function cancelEdit(): void {
    editing = false;
    draft = "";
    visible = false;
    validationKey = null;
    rawSaveError = null;
  }

  async function commit(): Promise<void> {
    if (!draft) {
      validationKey = "settings.credential.validation_empty";
      return;
    }
    saving = true;
    validationKey = null;
    rawSaveError = null;
    try {
      await setCredential(toolName, keyName, draft);
      editing = false;
      draft = "";
    } catch (err) {
      rawSaveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function remove(): Promise<void> {
    saving = true;
    rawSaveError = null;
    try {
      await deleteCredential(toolName, keyName);
      testResult = null;
    } catch (err) {
      rawSaveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function runTest(): Promise<void> {
    testing = true;
    testResult = null;
    try {
      testResult = await testCredential(toolName);
    } catch (err) {
      testResult = {
        ok: false,
        latency_ms: null,
        error: err instanceof Error ? err.message : String(err),
      };
    } finally {
      testing = false;
    }
  }
</script>

<div class="space-y-2" data-testid={dataTestId}>
  <div class="flex items-center justify-between gap-2">
    <label class="text-label-md text-foreground" for="{dataTestId}-input">{label}</label>
    {#if configured && !editing}
      <span
        class="inline-flex items-center gap-1 rounded-md bg-success/10 px-2 py-0.5 text-caption text-success-a11y"
      >
        <Check size={12} strokeWidth={2.5} aria-hidden="true" />
        {$t("settings.credential.configured")}
      </span>
    {:else if !configured && !editing}
      <span
        class="inline-flex items-center gap-1 rounded-md bg-warning/10 px-2 py-0.5 text-caption text-warning-a11y"
      >
        <AlertTriangle size={12} strokeWidth={2} aria-hidden="true" />
        {$t("settings.credential.absent")}
      </span>
    {/if}
  </div>

  {#if !editing}
    <div class="flex items-center gap-2">
      <code
        class="flex-1 rounded-md border border-border bg-muted/40 px-3 py-2 text-body-sm text-muted-foreground"
      >
        {configured ? "••••••••••••" : $t("settings.credential.not_configured")}
      </code>
      <Button variant="outline" size="sm" onclick={startEdit} data-testid="{dataTestId}-edit">
        {configured ? $t("settings.credential.edit") : $t("settings.credential.add")}
      </Button>
      {#if configured}
        <Button
          variant="ghost"
          size="sm"
          onclick={remove}
          disabled={saving}
          data-testid="{dataTestId}-delete"
        >
          {$t("settings.credential.delete")}
        </Button>
        {#if canTest}
          <Button
            variant="outline"
            size="sm"
            onclick={runTest}
            loading={testing}
            disabled={testing}
            data-testid="{dataTestId}-test"
          >
            {$t("settings.credential.test")}
          </Button>
        {/if}
      {/if}
    </div>
  {:else}
    <div class="flex items-center gap-2">
      <div class="relative flex-1">
        <Input
          id="{dataTestId}-input"
          type={visible ? "text" : "password"}
          bind:value={draft}
          autocomplete="off"
          spellcheck={false}
          class="w-full pr-10 text-body-sm"
          placeholder={placeholder ?? $t("settings.credential.placeholder")}
          data-testid="{dataTestId}-input"
        />
        <Button
          variant="ghost"
          size="icon-sm"
          type="button"
          class="absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
          aria-label={visible ? $t("settings.credential.hide") : $t("settings.credential.reveal")}
          onclick={() => (visible = !visible)}
        >
          {#if visible}
            <EyeOff size={16} aria-hidden="true" />
          {:else}
            <Eye size={16} aria-hidden="true" />
          {/if}
        </Button>
      </div>
      <Button
        variant="default"
        size="sm"
        onclick={commit}
        loading={saving}
        disabled={!draft || saving}
        data-testid="{dataTestId}-save"
      >
        {$t("common.save")}
      </Button>
      <Button variant="ghost" size="sm" onclick={cancelEdit} disabled={saving}>
        {$t("common.cancel")}
      </Button>
    </div>
  {/if}

  {#if validationKey}
    <p class="flex items-start gap-1.5 text-caption text-danger-a11y" data-testid="{dataTestId}-validation">
      <AlertTriangle size={13} strokeWidth={2} class="mt-px shrink-0" aria-hidden="true" />
      <span>{$t(validationKey)}</span>
    </p>
  {/if}

  {#if saveError}
    <CredentialErrorNotice error={saveError} data-testid="{dataTestId}-error" />
  {/if}

  {#if testResult}
    {#if testResult.ok}
      <p
        class="flex items-center gap-1 text-caption text-success-a11y"
        data-testid="{dataTestId}-test-result"
      >
        <Check size={12} strokeWidth={2.5} aria-hidden="true" />
        {testResult.latency_ms != null
          ? $t("settings.credential.test_valid_ms", { values: { ms: testResult.latency_ms } })
          : $t("settings.credential.test_valid")}
      </p>
    {:else if testError}
      <CredentialErrorNotice error={testError} data-testid="{dataTestId}-test-result" />
    {/if}
  {/if}
</div>
