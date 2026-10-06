<script lang="ts">
  /**
   * ServerForm - the form of a server, for adding one and for changing one:
   * its name, its address and its key, which is never required. "Test" lists
   * the models of the server without saving anything; "Save" stays available
   * when the test did not succeed, with a warning, so a server that is off
   * while it is typed can still be saved.
   *
   * Developer mode adds how long the server may stay silent and the fields
   * added to every request sent to it.
   */
  import { t } from "svelte-i18n";
  import type { DeclaredServer } from "$lib/ipc/llmServers";
  import { developerMode } from "$lib/stores/developerMode";
  import { reportError } from "$lib/errors/reportError";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { FormField } from "$lib/components/ui/form-field";
  import { Input } from "$lib/components/ui/input";
  import { Spinner } from "$lib/components/ui/progress";
  import { Textarea } from "$lib/components/ui/textarea";
  import { SERVER_ADDRESS_EXAMPLE, SERVER_FAILURE_TEXT } from "../onboarding/serverSetup";
  import {
    SERVER_FORM_ERROR_TEXT,
    SERVER_TIMEOUT_MAX_SECS,
    SERVER_TIMEOUT_MIN_SECS,
    seedServerForm,
    suggestedServerName,
    validateServerForm,
    type ServerFormErrors,
    type ServerTestOutcome,
    type Servers,
  } from "./servers";

  interface Props {
    servers: Servers;
    /** The server being changed, `null` for a new one. */
    editing: DeclaredServer | null;
    onsaved: (server: DeclaredServer) => void;
    oncancel: () => void;
  }

  let { servers, editing, onsaved, oncancel }: Props = $props();

  // svelte-ignore state_referenced_locally
  let form = $state(seedServerForm(editing));
  let nameTouched = $state(false);
  let submitted = $state(false);
  let testing = $state(false);
  let saving = $state(false);
  let outcome = $state<ServerTestOutcome | null>(null);

  const takenNames = $derived($servers.servers.map((server) => server.name));
  const errors = $derived<ServerFormErrors>(
    validateServerForm(form, { editing: editing !== null, developer: $developerMode, takenNames }),
  );
  const valid = $derived(Object.keys(errors).length === 0);

  function errorOf(field: keyof ServerFormErrors): string | undefined {
    const reason = errors[field];
    return submitted && reason ? $t(SERVER_FORM_ERROR_TEXT[reason]) : undefined;
  }

  /** The address or the key changed: what was tested no longer describes it. */
  function edited(): void {
    outcome = null;
    if (editing === null && !nameTouched) form.name = suggestedServerName(form.address);
  }

  async function test(): Promise<void> {
    if (testing) return;
    testing = true;
    outcome = null;
    try {
      outcome = await servers.testForm(form, editing);
    } finally {
      testing = false;
    }
  }

  async function save(event?: Event): Promise<void> {
    event?.preventDefault();
    submitted = true;
    if (!valid || saving) return;
    saving = true;
    try {
      onsaved(await servers.saveForm(form, editing, $developerMode));
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      saving = false;
    }
  }
</script>

<section class="flex max-w-2xl flex-col gap-4" data-testid="ai-server-form">
  <header class="space-y-1">
    <h2 class="text-sm font-semibold text-foreground">
      {editing
        ? $t("ai.servers.form.title_edit", { values: { name: editing.name } })
        : $t("ai.servers.form.title_add")}
    </h2>
    <p class="text-sm text-muted-foreground">{$t("ai.servers.form.intro")}</p>
  </header>

  <form class="flex flex-col gap-4" novalidate onsubmit={save}>
    <FormField
      id="ai-server-address"
      label={$t("ai.servers.form.address_label")}
      hint={$t("ai.servers.form.address_hint", { values: { example: SERVER_ADDRESS_EXAMPLE } })}
      error={errorOf("address")}
    >
      <Input
        id="ai-server-address"
        type="url"
        bind:value={form.address}
        oninput={edited}
        placeholder={SERVER_ADDRESS_EXAMPLE}
        autocomplete="off"
        spellcheck={false}
        data-testid="ai-server-address"
      />
    </FormField>

    <FormField
      id="ai-server-name"
      label={$t("ai.servers.form.name_label")}
      hint={editing ? $t("ai.servers.form.name_fixed_hint") : $t("ai.servers.form.name_hint")}
      error={errorOf("name")}
    >
      <Input
        id="ai-server-name"
        type="text"
        bind:value={form.name}
        oninput={() => (nameTouched = true)}
        disabled={editing !== null}
        autocomplete="off"
        spellcheck={false}
        data-testid="ai-server-name"
      />
    </FormField>

    <FormField
      id="ai-server-key"
      label={$t("ai.servers.form.key_label")}
      hint={editing?.has_key
        ? $t("ai.servers.form.key_saved_hint")
        : $t("ai.servers.form.key_hint")}
      optional
    >
      <Input
        id="ai-server-key"
        type="password"
        bind:value={form.apiKey}
        oninput={edited}
        placeholder={editing?.has_key
          ? $t("ai.servers.form.key_saved_placeholder")
          : $t("ai.servers.form.key_placeholder")}
        autocomplete="off"
        spellcheck={false}
        data-testid="ai-server-key"
      />
    </FormField>

    {#if $developerMode}
      <FormField
        id="ai-server-timeout"
        label={$t("dev.ai.servers.timeout_label")}
        hint={$t("dev.ai.servers.timeout_hint", {
          values: { min: SERVER_TIMEOUT_MIN_SECS, max: SERVER_TIMEOUT_MAX_SECS },
        })}
        error={errorOf("timeoutSecs")}
        optional
      >
        <Input
          id="ai-server-timeout"
          type="text"
          inputmode="numeric"
          bind:value={form.timeoutSecs}
          autocomplete="off"
          data-testid="ai-server-timeout"
        />
      </FormField>

      <FormField
        id="ai-server-extra"
        label={$t("dev.ai.servers.extra_label")}
        hint={$t("dev.ai.servers.extra_hint")}
        error={errorOf("extraJson")}
        optional
      >
        <Textarea
          id="ai-server-extra"
          bind:value={form.extraJson}
          class="font-mono"
          spellcheck={false}
          data-testid="ai-server-extra"
        />
      </FormField>
    {/if}

    <div class="flex flex-wrap items-center gap-2">
      <Button
        type="button"
        variant="outline"
        size="sm"
        onclick={test}
        disabled={testing || form.address.trim() === ""}
        data-testid="ai-server-test"
      >
        {#if testing}<Spinner size={12} />{/if}
        {testing ? $t("ai.servers.form.testing") : $t("ai.servers.form.test")}
      </Button>
      <Button type="submit" variant="primary-solid" size="sm" disabled={saving} data-testid="ai-server-save">
        {#if saving}<Spinner size={12} />{/if}
        {$t("ai.servers.form.save")}
      </Button>
      <Button type="button" variant="ghost" size="sm" onclick={oncancel} data-testid="ai-server-cancel">
        {$t("common.cancel")}
      </Button>
    </div>
  </form>

  {#if outcome === null && !testing}
    <p class="text-xs text-muted-foreground" data-testid="ai-server-untested-hint">
      {$t("ai.servers.form.untested_hint")}
    </p>
  {/if}

  {#if outcome?.phase === "listed"}
    <div class="space-y-2" data-testid="ai-server-test-ok">
      <Banner variant="success" surface="card">
        {$t("ai.servers.form.test_ok", { values: { count: outcome.models.length } })}
      </Banner>
      <ul
        class="divide-y divide-border rounded-lg border border-border text-sm"
        aria-label={$t("ai.servers.models_label")}
      >
        {#each outcome.models as model (model)}
          <li class="px-3 py-2 font-medium text-foreground" data-testid="ai-server-test-model">{model}</li>
        {/each}
      </ul>
    </div>
  {:else if outcome?.phase === "failed"}
    {@const text = SERVER_FAILURE_TEXT[outcome.failure.kind]}
    <Banner
      variant="destructive"
      surface="card"
      title={$t(text.title)}
      data-testid="ai-server-test-error"
    >
      <p>{$t(text.body)}</p>
      {#if outcome.failure.detail}
        <p class="mt-1 break-words text-xs opacity-80" data-testid="ai-server-test-error-detail">
          {outcome.failure.detail}
        </p>
      {/if}
    </Banner>
    <Banner variant="warning" surface="card" data-testid="ai-server-save-anyway">
      {$t("ai.servers.form.save_anyway")}
    </Banner>
  {/if}
</section>
