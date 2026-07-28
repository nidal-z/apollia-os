<script lang="ts">
  /**
   * OauthCredentialRow - one OAuth client credential inside an
   * OauthProviderCard. "public" mode (client_id) echoes the value and shows its
   * resolution source; empty save clears the override. "secret" mode
   * (client_secret / api_key) is write-only: never echoed, reveal toggle on
   * edit, save requires a value, removal via explicit Delete. Errors are
   * humanized (raw behind a Disclosure); the draft clears on success or cancel.
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Eye, EyeOff, AlertTriangle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { humanize } from "$lib/errors/humanize";
  import CredentialErrorNotice from "../settings/CredentialErrorNotice.svelte";
  import OauthCredentialHeader from "./OauthCredentialHeader.svelte";
  import type { OauthSource } from "$lib/ipc/oauthClients";

  interface Props {
    label: string;
    icon: Snippet;
    mode: "public" | "secret";
    required?: boolean;
    source: OauthSource;
    configured: boolean;
    currentValue?: string;
    placeholder: string;
    help?: string;
    onsave: (value: string) => Promise<void>;
    ondelete?: () => Promise<void>;
    inputTestid?: string;
    saveTestid?: string;
    deleteTestid?: string;
    "data-testid"?: string;
  }

  let {
    label,
    icon,
    mode,
    required = false,
    source,
    configured,
    currentValue = "",
    placeholder,
    help,
    onsave,
    ondelete,
    inputTestid,
    saveTestid,
    deleteTestid,
    "data-testid": testid,
  }: Props = $props();

  let editing = $state(false);
  let draft = $state("");
  let visible = $state(false);
  let saving = $state(false);
  let validationKey = $state<string | null>(null);
  let rawError = $state<string | null>(null);

  const humanized = $derived(rawError ? humanize(rawError, $t) : null);
  // Public (client_id) rows clear via an empty save, so they never pass
  // `ondelete`; secret rows expose Delete only once a value exists.
  const showDelete = $derived(
    ondelete != null && (mode === "secret" ? configured : true),
  );

  function startEdit(): void {
    editing = true;
    draft = "";
    visible = false;
    validationKey = null;
    rawError = null;
  }

  function cancelEdit(): void {
    editing = false;
    draft = "";
    visible = false;
    validationKey = null;
  }

  async function commit(): Promise<void> {
    const value = draft.trim();
    if (mode === "secret" && value.length === 0) {
      validationKey = "settings.credential.validation_empty";
      return;
    }
    saving = true;
    validationKey = null;
    rawError = null;
    try {
      await onsave(value);
      editing = false;
      draft = "";
      visible = false;
    } catch (err) {
      rawError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function remove(): Promise<void> {
    if (!ondelete) return;
    saving = true;
    rawError = null;
    try {
      await ondelete();
    } catch (err) {
      rawError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void commit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelEdit();
    }
  }
</script>

<div
  class="border-b border-border/60 py-3.5 last:border-b-0"
  data-testid={testid}
>
  <OauthCredentialHeader {label} {icon} {required} {mode} {source} {configured} />

  {#if !editing}
    <div class="flex flex-wrap items-center gap-2">
      <code
        class="min-w-0 flex-1 truncate rounded-md border border-border/60 bg-muted/40 px-3 py-2 font-mono text-body-sm {configured
          ? 'text-muted-foreground'
          : 'italic text-muted-foreground/70'}"
      >
        {#if mode === "secret"}
          {configured ? "••••••••••••••••" : $t("settings.credential.not_configured")}
        {:else}
          {currentValue || $t("settings.credential.not_configured")}
        {/if}
      </code>
      <Button
        variant={configured ? "outline" : "default"}
        size="sm"
        onclick={startEdit}
        data-testid={inputTestid ? `${inputTestid}-edit` : undefined}
      >
        {#if configured}
          {source === "builtin"
            ? $t("settings.credential.override")
            : $t("settings.credential.edit")}
        {:else}
          {$t("settings.credential.add")}
        {/if}
      </Button>
      {#if showDelete}
        <Button
          variant="ghost"
          size="sm"
          class="text-destructive hover:bg-destructive/10"
          onclick={remove}
          disabled={saving}
          data-testid={deleteTestid}
        >
          {$t("settings.credential.delete")}
        </Button>
      {/if}
    </div>
  {:else}
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative min-w-0 flex-1">
        <Input
          type={mode === "secret" && !visible ? "password" : "text"}
          bind:value={draft}
          {placeholder}
          autocomplete="off"
          spellcheck={false}
          onkeydown={onKeydown}
          disabled={saving}
          class="w-full pr-10 font-mono text-body-sm {rawError
            ? 'border-destructive focus-visible:ring-destructive'
            : ''}"
          data-testid={inputTestid}
        />
        {#if mode === "secret"}
          <Button
            variant="ghost"
            size="icon-sm"
            type="button"
            class="absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            aria-label={visible
              ? $t("settings.credential.hide")
              : $t("settings.credential.reveal")}
            onclick={() => (visible = !visible)}
          >
            {#if visible}
              <EyeOff size={15} aria-hidden="true" />
            {:else}
              <Eye size={15} aria-hidden="true" />
            {/if}
          </Button>
        {/if}
      </div>
      <Button
        variant="default"
        size="sm"
        onclick={commit}
        loading={saving}
        disabled={saving || (mode === "secret" && draft.trim().length === 0)}
        data-testid={saveTestid}
      >
        {$t("common.save")}
      </Button>
      <Button variant="ghost" size="sm" onclick={cancelEdit} disabled={saving}>
        {$t("common.cancel")}
      </Button>
    </div>
  {/if}

  {#if help}
    <p class="mt-2 max-w-prose text-caption text-muted-foreground">{help}</p>
  {/if}

  {#if validationKey}
    <p
      class="mt-2 flex items-start gap-1.5 text-caption text-danger-a11y"
      data-testid={inputTestid ? `${inputTestid}-validation` : undefined}
    >
      <AlertTriangle size={13} strokeWidth={2} class="mt-px shrink-0" aria-hidden="true" />
      <span>{$t(validationKey)}</span>
    </p>
  {/if}

  {#if humanized}
    <div class="mt-2">
      <CredentialErrorNotice
        error={humanized}
        data-testid={inputTestid ? `${inputTestid}-error` : undefined}
      />
    </div>
  {/if}
</div>
