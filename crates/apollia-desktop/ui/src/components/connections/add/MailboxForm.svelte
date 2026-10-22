<script lang="ts">
  /**
   * MailboxForm - the connect step of a mailbox: the address, an application
   * password, and one Add that logs in for real before anything is kept.
   *
   * The server and port are filled from the address and stay editable under
   * Advanced; once edited there, the address no longer overwrites them. A
   * failed login keeps nothing and says why in plain words. The password lives
   * in this form only while it is shown.
   */
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { FormField } from "$lib/components/ui/form-field";
  import { Banner } from "$lib/components/ui/banner";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { Spinner } from "$lib/components/ui/progress";
  import { listImapProviders, type ImapProviders } from "$lib/ipc/connections";
  import { IMAP_TLS_PORT, addMailbox, canAdd, prefillFor } from "$lib/integrations/mailbox";
  import type { Refusal } from "$lib/integrations/addConnection";

  interface Props {
    /** The address to start from, empty for a new mailbox. */
    initialEmail?: string;
    oncancel: () => void;
    /** The mailbox is kept: its connection id. */
    onadded: (connectionId: string) => void;
    onrefused: (refusal: Refusal) => void;
  }

  let { initialEmail = "", oncancel, onadded, onrefused }: Props = $props();

  // svelte-ignore state_referenced_locally
  let email = $state(initialEmail);
  let password = $state("");
  let host = $state("");
  let port = $state(String(IMAP_TLS_PORT));
  let serverEdited = $state(false);
  let known = $state<ImapProviders | null>(null);
  let trying = $state(false);
  let errorKey = $state<string | null>(null);
  let errorDetail = $state<string | null>(null);

  $effect(() => {
    let cancelled = false;
    void listImapProviders()
      .then((providers) => {
        if (!cancelled) known = providers;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  const prefill = $derived(prefillFor(email, known));

  // The server and port follow the address until the person edits them.
  $effect(() => {
    if (serverEdited) return;
    host = prefill?.host ?? "";
    port = String(prefill?.port ?? known?.defaultPort ?? IMAP_TLS_PORT);
  });

  const form = $derived({ email, password, host, port });
  const ready = $derived(canAdd(form));

  async function add(): Promise<void> {
    if (!ready || trying) return;
    trying = true;
    errorKey = null;
    errorDetail = null;
    const outcome = await addMailbox(form);
    trying = false;
    if (outcome.kind === "added") {
      password = "";
      onadded(outcome.account.connectionId);
      return;
    }
    if (outcome.kind === "refused") {
      password = "";
      onrefused(outcome.refusal);
      return;
    }
    errorKey = outcome.messageKey;
    errorDetail = outcome.detail;
  }

  function cancel(): void {
    password = "";
    oncancel();
  }
</script>

<div class="space-y-4" data-testid="mailbox-body">
  <FormField
    id="mailbox-email"
    label={$t("connections.mailbox.email_label")}
    hint={$t("connections.mailbox.email_hint")}
  >
    <Input
      id="mailbox-email"
      type="email"
      autocomplete="email"
      bind:value={email}
      placeholder={$t("connections.mailbox.email_placeholder")}
      disabled={trying}
      data-testid="mailbox-email"
    />
  </FormField>

  <FormField
    id="mailbox-password"
    label={$t("connections.mailbox.password_label")}
    hint={$t("connections.mailbox.password_hint")}
  >
    <Input
      id="mailbox-password"
      type="password"
      autocomplete="off"
      bind:value={password}
      disabled={trying}
      data-testid="mailbox-password"
    />
  </FormField>

  <Disclosure open={errorKey !== null} controlled testid="mailbox-advanced">
    {#snippet summary()}
      <span class="font-medium text-foreground">{$t("connections.mailbox.advanced")}</span>
    {/snippet}
    {#snippet children()}
      <p class="mb-3 text-caption text-muted-foreground">{$t("connections.mailbox.advanced_hint")}</p>
      <div class="grid grid-cols-3 gap-3">
        <FormField id="mailbox-host" label={$t("connections.mailbox.host_label")} class="col-span-2">
          <Input
            id="mailbox-host"
            bind:value={host}
            oninput={() => (serverEdited = true)}
            disabled={trying}
            data-testid="mailbox-host"
          />
        </FormField>
        <FormField id="mailbox-port" label={$t("connections.mailbox.port_label")}>
          <Input
            id="mailbox-port"
            inputmode="numeric"
            bind:value={port}
            oninput={() => (serverEdited = true)}
            disabled={trying}
            data-testid="mailbox-port"
          />
        </FormField>
      </div>
    {/snippet}
  </Disclosure>

  {#if errorKey}
    <Banner variant="destructive" surface="card" data-testid="mailbox-error">
      <span data-testid="mailbox-error-message">{$t(errorKey)}</span>
      {#if errorDetail}
        <details class="mt-1 text-xs">
          <summary class="cursor-pointer text-muted-foreground">
            {$t("integrations.wizard.test_err.raw_details")}
          </summary>
          <p class="mt-1 break-all font-mono text-caption">{errorDetail}</p>
        </details>
      {/if}
    </Banner>
  {/if}

  {#if trying}
    <div class="flex items-center gap-2 text-sm text-foreground" data-testid="mailbox-trying">
      <Spinner size={14} />
      {$t("connections.mailbox.trying")}
    </div>
  {/if}

  <div class="flex items-center justify-end gap-2 border-t border-border pt-4">
    <p class="mr-auto text-caption text-muted-foreground">{$t("connections.mailbox.add_hint")}</p>
    <Button variant="outline" size="sm" onclick={cancel} data-testid="mailbox-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="primary-solid"
      size="sm"
      onclick={() => void add()}
      disabled={!ready || trying}
      data-testid="mailbox-add"
    >
      {$t("connections.mailbox.add")}
    </Button>
  </div>
</div>
