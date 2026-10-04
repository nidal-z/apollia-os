<script lang="ts">
  /**
   * Adding a mailbox: the address, an application password, and one "Add"
   * button that tries the connection for real before keeping it.
   *
   * The server and port are filled from the address and stay editable under
   * Advanced; once edited there, the address no longer overwrites them. A
   * failed trial keeps nothing and says why in plain words; a refusal says who
   * refused. The password lives in this dialog only while it is open.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle2, Mail } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { FormField } from "$lib/components/ui/form-field";
  import { Banner } from "$lib/components/ui/banner";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { Spinner } from "$lib/components/ui/progress";
  import ConnectionRefusal from "./ConnectionRefusal.svelte";
  import { listImapProviders, type ImapProviders } from "$lib/ipc/connections";
  import {
    IMAP_TLS_PORT,
    addMailbox,
    canAdd,
    prefillFor,
  } from "$lib/integrations/mailbox";
  import type { Refusal } from "$lib/integrations/addConnection";
  import { openSettingsWindow } from "$lib/stores/settingsWindow";

  interface Props {
    open: boolean;
    onclose: () => void;
    /** Called with the connection id once the mailbox is kept, so the list shows it. */
    onadded: (connectionId: string) => void;
  }

  let { open, onclose, onadded }: Props = $props();

  type Phase = "form" | "trying" | "added" | "refused";

  let phase = $state<Phase>("form");
  let email = $state("");
  let password = $state("");
  let host = $state("");
  let port = $state(String(IMAP_TLS_PORT));
  /** Set once the person edits the server or port under Advanced. */
  let serverEdited = $state(false);
  let known = $state<ImapProviders | null>(null);
  let errorKey = $state<string | null>(null);
  let errorDetail = $state<string | null>(null);
  let refusal = $state<Refusal | null>(null);
  let addedId = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    phase = "form";
    email = "";
    password = "";
    host = "";
    port = String(IMAP_TLS_PORT);
    serverEdited = false;
    errorKey = null;
    errorDetail = null;
    refusal = null;
    addedId = null;
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

  function editServer(): void {
    serverEdited = true;
  }

  async function add(): Promise<void> {
    if (!ready || phase === "trying") return;
    phase = "trying";
    errorKey = null;
    errorDetail = null;
    const outcome = await addMailbox(form);
    if (outcome.kind === "added") {
      password = "";
      addedId = outcome.account.connectionId;
      phase = "added";
      onadded(outcome.account.connectionId);
      return;
    }
    if (outcome.kind === "refused") {
      password = "";
      refusal = outcome.refusal;
      phase = "refused";
      return;
    }
    errorKey = outcome.messageKey;
    errorDetail = outcome.detail;
    phase = "form";
  }

  function close(): void {
    password = "";
    onclose();
  }

  function openRefusingRule(): void {
    close();
    openSettingsWindow("permissions");
  }
</script>

<Dialog
  {open}
  onclose={close}
  size="md"
  label={$t("connections.mailbox.title")}
  data-testid="mailbox-dialog"
>
  <div class="space-y-4" data-phase={phase} data-testid="mailbox-body">
    <div class="flex items-start gap-3">
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-surface-2 text-primary"
        aria-hidden="true"
      >
        <Mail size={16} />
      </div>
      <div class="min-w-0">
        <h2 class="text-body-lg font-medium text-foreground">{$t("connections.mailbox.title")}</h2>
        <p class="text-caption text-muted-foreground">{$t("connections.mailbox.subtitle")}</p>
      </div>
    </div>

    {#if phase === "added"}
      <div
        class="rounded-md border border-success/30 bg-success/10 px-4 py-3"
        data-testid="mailbox-added"
        data-connection-id={addedId}
      >
        <div class="flex items-center gap-2">
          <CheckCircle2 size={16} class="shrink-0 text-success" aria-hidden="true" />
          <p class="text-sm font-medium text-success-a11y">
            {$t("connections.mailbox.added", { values: { email: email.trim() } })}
          </p>
        </div>
        <p class="mt-1 text-xs text-muted-foreground">{$t("connections.mailbox.added_where")}</p>
      </div>
    {:else if phase === "refused" && refusal}
      <ConnectionRefusal {refusal} onopen={openRefusingRule} />
    {:else}
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
          disabled={phase === "trying"}
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
          disabled={phase === "trying"}
          data-testid="mailbox-password"
        />
      </FormField>

      <Disclosure open={errorKey !== null} controlled testid="mailbox-advanced">
        {#snippet summary()}
          <span class="font-medium text-foreground">{$t("connections.mailbox.advanced")}</span>
        {/snippet}
        {#snippet children()}
          <p class="mb-3 text-caption text-muted-foreground">
            {$t("connections.mailbox.advanced_hint")}
          </p>
          <div class="grid grid-cols-3 gap-3">
            <FormField
              id="mailbox-host"
              label={$t("connections.mailbox.host_label")}
              class="col-span-2"
            >
              <Input
                id="mailbox-host"
                bind:value={host}
                oninput={editServer}
                disabled={phase === "trying"}
                data-testid="mailbox-host"
              />
            </FormField>
            <FormField id="mailbox-port" label={$t("connections.mailbox.port_label")}>
              <Input
                id="mailbox-port"
                inputmode="numeric"
                bind:value={port}
                oninput={editServer}
                disabled={phase === "trying"}
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

      {#if phase === "trying"}
        <div class="flex items-center gap-2 text-sm text-foreground" data-testid="mailbox-trying">
          <Spinner size={14} />
          {$t("connections.mailbox.trying")}
        </div>
      {/if}
    {/if}
  </div>

  <div class="mt-6 flex items-center justify-end gap-2 border-t border-border pt-4">
    {#if phase === "form" || phase === "trying"}
      <p class="mr-auto text-caption text-muted-foreground">{$t("connections.mailbox.add_hint")}</p>
      <Button variant="outline" size="sm" onclick={close} data-testid="mailbox-cancel">
        {$t("common.cancel")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => void add()}
        disabled={!ready || phase === "trying"}
        data-testid="mailbox-add"
      >
        {$t("connections.mailbox.add")}
      </Button>
    {:else}
      <Button variant="primary-solid" size="sm" onclick={close} data-testid="mailbox-done">
        {$t("common.close")}
      </Button>
    {/if}
  </div>
</Dialog>
