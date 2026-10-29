<script lang="ts">
  /**
   * AddFlow - the one flow that adds a connection, whatever its kind, in
   * three steps: connect it (accept the warning of an entry and give what it
   * asks, type an address, a mailbox, pick a folder, sign in to an account),
   * say who may use it, then try it.
   *
   * Cancel before the connection exists goes back to where the flow started.
   * Once it exists, closing keeps it: the list shows it whatever the trial
   * answered, and a failed trial offers Retry and Remove.
   */
  import { onMount, untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { ArrowLeft, Folder, LogIn, Play, XCircle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { ProgressBar, Spinner } from "$lib/components/ui/progress";
  import { developerMode } from "$lib/stores/developerMode";
  import { navigateTo } from "$lib/stores/navigation";
  import {
    addMcpServer,
    listAgents,
    removeMcpServer,
    type PendingSecret,
    type ProviderId,
  } from "$lib/ipc/connections";
  import {
    addConnection,
    MIN_NODE_MAJOR,
    NODE_HELP_PATH,
    nodePercent,
    refusalOf,
    tryAddedConnection,
    type NodeRuntimeState,
    type Refusal,
  } from "$lib/integrations/addConnection";
  import { chooseAndAddFolder } from "$lib/connections/folderConnection";
  import {
    FLOW_STEPS,
    grantToAgents,
    initiallyChecked,
    keptOnceAdded,
    mcpConnectionId,
    stepOf,
    type FlowPhase,
  } from "$lib/connections/addFlow";
  import type { AddEntry } from "$lib/connections/addWindow";
  import type { AgentListItem, Connection, McpServerConfigInput, NodeSource } from "$lib/types";
  import { docsUrl } from "$lib/utils/docsUrl";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import WizardStepDisclaimer from "../../integrations/WizardStepDisclaimer.svelte";
  import WizardStepAuth from "../../integrations/WizardStepAuth.svelte";
  import WizardStepCoaching from "../../integrations/WizardStepCoaching.svelte";
  import ConnectionRefusal from "../../integrations/ConnectionRefusal.svelte";
  import { createCatalogueSetup } from "./catalogueSetup.svelte";
  import AddressForm from "./AddressForm.svelte";
  import MailboxForm from "./MailboxForm.svelte";
  import WhoCanUse from "./WhoCanUse.svelte";
  import TrialSucceeded from "./TrialSucceeded.svelte";

  interface Props {
    entry: AddEntry;
    /** The name the entry is shown under. */
    name: string;
    /** The card before the flow showed the warning, and Add accepted it. */
    accepted?: boolean;
    /** Start on "who may use it" with this connection: a sign-in done in its own dialog. */
    resume?: string | null;
    /** The address of a mailbox to start from. */
    mailboxAddress?: string;
    /** Cancel before the connection exists. */
    onback: () => void;
    /** The connection exists, so the list can show it. */
    onadded?: (connectionId: string) => void;
    /** The flow is over; the connection, when one was added. */
    ondone: (connectionId: string | null) => void;
    /** Sign in to a native account, in the dialog of its provider. */
    onsignin?: (provider: ProviderId) => void;
    /** Try the connection added, in the Chat. */
    ontry?: (connectionId: string, name: string) => void;
    /** Try one of the examples of an entry of the catalogue. */
    ontryprompt?: (connectionId: string, prompt: string) => void;
    /** The refusing rule is opened, on the Authorizations screen: close what holds the flow. */
    onopenrule: () => void;
  }

  let {
    entry,
    name,
    accepted = false,
    resume = null,
    mailboxAddress = "",
    onback,
    onadded = () => {},
    ondone,
    onsignin,
    ontry,
    ontryprompt,
    onopenrule,
  }: Props = $props();

  const setup = untrack(() => (entry.kind === "catalogue" ? createCatalogueSetup(entry.entry) : null));

  let phase = $state<FlowPhase>(untrack(() => (resume ? "who" : "connect")));
  let warningAccepted = $state(untrack(() => accepted));
  let connectionId = $state<string | null>(untrack(() => resume));
  /**
   * The configuration of the server being added, read again by Retry; once
   * added, only what the trial and the removal read (`keptOnceAdded`).
   */
  let serverConfig = $state<McpServerConfigInput | null>(null);
  let folder = $state<Connection | null>(null);
  let nodeState = $state<NodeRuntimeState | null>(null);
  let working = $state<"adding" | "probing" | "granting" | "trying">("adding");
  let refusal = $state<Refusal | null>(null);
  let failure = $state<string | null>(null);
  let failureNode = $state(false);
  let agents = $state<AgentListItem[]>([]);
  let agentsLoading = $state(false);
  let checked = $state<string[]>(initiallyChecked());
  let grantError = $state<string | null>(null);
  let trialOk = $state(true);
  let trialTools = $state<number | null>(null);
  let trialMessageKey = $state<string | null>(null);
  let trialDetail = $state<string | null>(null);
  let trialNodeSource = $state<NodeSource | null>(null);
  let removing = $state(false);
  let removeError = $state<string | null>(null);

  const step = $derived(stepOf(phase));
  const STEP_KEYS = {
    connect: "connections.add_flow.steps.connect",
    who: "connections.add_flow.steps.who",
    trial: "connections.add_flow.steps.trial",
  } as const;

  function messageOf(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  onMount(() => {
    if (resume) void loadAgents();
    else if (setup && accepted) void acceptWarning();
  });

  async function loadAgents(): Promise<void> {
    agentsLoading = true;
    try {
      agents = await listAgents();
    } catch {
      agents = [];
    } finally {
      agentsLoading = false;
    }
  }

  /** The connection exists: on to who may use it. */
  function added(id: string): void {
    connectionId = id;
    onadded(id);
    phase = "who";
    void loadAgents();
  }

  function openRefusingRule(): void {
    onopenrule();
    navigateTo("authorizations");
  }

  function refused(r: Refusal): void {
    refusal = r;
    phase = "refused";
  }

  // ── Catalogue ───────────────────────────────────────────────────────────
  async function acceptWarning(): Promise<void> {
    if (!setup) return;
    warningAccepted = true;
    phase = "working";
    working = "probing";
    await setup.probe();
    if (setup.needsInput) {
      phase = "connect";
      return;
    }
    await addServer(setup.config(), setup.secrets(), true);
  }

  /** Add a server, its warning accepted when it comes from the catalogue. */
  async function addServer(
    config: McpServerConfigInput | null,
    secrets: PendingSecret[],
    fromCatalogue: boolean,
  ): Promise<void> {
    if (!config) {
      failure = $t("integrations.wizard.no_package_body");
      phase = "failed";
      return;
    }
    serverConfig = config;
    phase = "working";
    working = "adding";
    nodeState = null;
    failure = null;
    failureNode = false;
    if (fromCatalogue) {
      const outcome = await addConnection(config, secrets, { onNode: (s) => (nodeState = s) });
      if (outcome.kind === "refused") return refused(outcome.refusal);
      if (outcome.kind === "failed") {
        failure = outcome.message;
        failureNode = outcome.reason === "node_unavailable";
        phase = "failed";
        return;
      }
    } else {
      try {
        await addMcpServer({ ...config });
      } catch (err: unknown) {
        const r = refusalOf(messageOf(err));
        if (r) return refused(r);
        failure = messageOf(err);
        phase = "failed";
        return;
      }
    }
    serverConfig = keptOnceAdded(config);
    added(mcpConnectionId(config.name));
  }

  // ── Folder ──────────────────────────────────────────────────────────────
  async function pickFolder(): Promise<void> {
    const outcome = await chooseAndAddFolder($t("connections.folder.picker_title"));
    if (outcome.kind === "cancelled") return;
    if (outcome.kind === "refused") return refused(outcome.refusal);
    if (outcome.kind === "failed") {
      failure = outcome.message;
      phase = "failed";
      return;
    }
    folder = outcome.connection;
    grantError = null;
    added(outcome.connection.id);
  }

  // ── Who, then the trial ─────────────────────────────────────────────────
  async function confirmWho(): Promise<void> {
    const id = connectionId;
    if (!id) return;
    phase = "working";
    working = "granting";
    grantError = null;
    try {
      await grantToAgents(id, checked);
    } catch (err: unknown) {
      grantError = messageOf(err);
    }
    await trial();
  }

  async function trial(): Promise<void> {
    if (!serverConfig) {
      trialOk = true;
      phase = "trial";
      return;
    }
    phase = "working";
    working = "trying";
    const outcome = await tryAddedConnection(serverConfig.name);
    trialOk = outcome.kind === "succeeded";
    trialNodeSource = outcome.nodeSource ?? null;
    if (outcome.kind === "succeeded") {
      trialTools = outcome.tools;
    } else {
      trialMessageKey = outcome.messageKey;
      trialDetail = outcome.detail;
    }
    phase = "trial";
  }

  async function removeAdded(): Promise<void> {
    if (!serverConfig) return;
    removing = true;
    removeError = null;
    try {
      await removeMcpServer(serverConfig.name);
      ondone(null);
    } catch (err: unknown) {
      removeError = messageOf(err);
    } finally {
      removing = false;
    }
  }

  function retryAdd(): void {
    if (setup) void addServer(setup.config(), setup.secrets(), true);
    else if (serverConfig) void addServer(serverConfig, [], false);
  }

  const nodeDownloading = $derived(nodeState?.state === "downloading");
  const nodeProgress = $derived(nodeState ? nodePercent(nodeState) : null);
  const WORKING_KEYS = {
    adding: "integrations.wizard.preparing",
    probing: "integrations.wizard.oauth_probing",
    granting: "connections.add_flow.granting",
    trying: "integrations.wizard.trying",
  } as const;
</script>

<div
  class="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-auto px-6 py-5"
  data-testid="connector-wizard"
  data-phase={phase}
  data-kind={entry.kind}
>
  <header class="flex flex-col gap-2">
    {#if phase === "connect" || phase === "refused" || phase === "failed"}
      <button
        type="button"
        class="-ml-1.5 flex w-fit items-center gap-1 rounded-md px-1.5 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        onclick={onback}
      >
        <ArrowLeft size={13} aria-hidden="true" />
        {$t("connections.add_window.back")}
      </button>
    {/if}
    <h2 class="min-w-0 truncate text-lg font-semibold text-foreground">{name}</h2>
    <ol class="flex flex-wrap items-center gap-2 text-caption" data-testid="add-flow-steps" data-step={step}>
      {#each FLOW_STEPS as s, i (s)}
        <li
          class="flex items-center gap-1.5 {s === step ? 'font-medium text-foreground' : 'text-muted-foreground'}"
          aria-current={s === step ? "step" : undefined}
        >
          <span
            class="flex h-5 w-5 items-center justify-center rounded-full border text-micro {s === step
              ? 'border-primary bg-primary/10 text-primary'
              : 'border-border'}">{i + 1}</span
          >
          {$t(STEP_KEYS[s])}
        </li>
      {/each}
    </ol>
  </header>

  <div class="min-h-40 space-y-4" data-testid="wizard-body">
    {#if phase === "connect"}
      {#if entry.kind === "catalogue" && setup}
        {#if !warningAccepted}
          <WizardStepDisclaimer {name} />
        {:else}
          <WizardStepAuth
            envVars={setup.envVars}
            enrichment={setup.server.enrichment}
            values={setup.envValues}
            onchange={setup.setEnv}
            packageArgs={setup.packageArgs}
            argValues={setup.argValues}
            onArgChange={setup.setArg}
            localLoopback={setup.localLoopback}
            probeMode={setup.probeMode}
            probeError={setup.probeError}
            oauthDiscovery={setup.discovery}
            oauthSelectedScopes={setup.scopes}
            oauthAccount={setup.account}
            oauthSigningIn={setup.signingIn}
            oauthSigninError={setup.signinError}
            oauthUsingPreRegisteredApp={setup.usingPreRegisteredApp}
            onOAuthToggleScope={setup.toggleScope}
            onOAuthSignin={() => void setup.signIn()}
          />
        {/if}
      {:else if entry.kind === "address"}
        <AddressForm oncancel={onback} onsubmit={(config) => void addServer(config, [], false)} />
      {:else if entry.kind === "mailbox"}
        <MailboxForm initialEmail={mailboxAddress} oncancel={onback} onadded={added} onrefused={refused} />
      {:else if entry.kind === "folder"}
        <p class="text-sm text-foreground">{$t("connections.add_flow.folder_body")}</p>
        <p class="text-caption text-muted-foreground">{$t("connections.folder.bounds")}</p>
      {:else if entry.kind === "native"}
        <p class="text-sm text-foreground">
          {$t("connections.add_flow.sign_in_body", { values: { name } })}
        </p>
        {#if entry.provider === "microsoft"}
          <p class="text-caption text-muted-foreground">{$t("connections.add_flow.sign_in_work_account")}</p>
        {/if}
      {/if}
    {:else if phase === "working"}
      <div class="space-y-3" data-testid="wizard-preparing">
        <div class="flex items-center gap-2 text-sm text-foreground">
          <Spinner size={14} />
          {#if nodeDownloading}
            {$developerMode
              ? $t("dev.integrations.wizard.node_downloading")
              : $t("integrations.wizard.component_downloading")}
          {:else}
            {$t(WORKING_KEYS[working])}
          {/if}
        </div>
        {#if nodeDownloading}
          <div data-testid="wizard-node-progress">
            <ProgressBar value={nodeProgress ?? undefined} />
            <p class="mt-1 text-xs text-muted-foreground">{$t("integrations.wizard.component_verified")}</p>
          </div>
        {/if}
      </div>
    {:else if phase === "who"}
      <WhoCanUse {agents} loading={agentsLoading} bind:checked />
    {:else if phase === "trial"}
      {#if grantError}
        <p class="text-caption text-destructive" role="alert" data-testid="wizard-grant-error">
          {$t("connections.add_flow.grant_failed", { values: { error: grantError } })}
        </p>
      {/if}
      {#if trialNodeSource === "machine"}
        <p class="text-caption text-muted-foreground" data-testid="add-flow-machine-node">
          {$t("integrations.wizard.node_machine")}
        </p>
      {/if}
      {#if trialOk}
        <TrialSucceeded
          kind={entry.kind}
          {name}
          tools={trialTools}
          {folder}
          onchooseanother={entry.kind === "folder" ? () => void pickFolder() : undefined}
        />
        {#if entry.kind === "catalogue" && connectionId}
          {@const id = connectionId}
          <WizardStepCoaching
            serverName={entry.entry.name}
            serverTitle={name}
            ontry={(prompt) => ontryprompt?.(id, prompt)}
          />
        {/if}
      {:else}
        <div class="space-y-2 rounded-md border border-destructive/30 bg-destructive/5 px-4 py-3" data-testid="test-result-error">
          <div class="flex items-center gap-2">
            <XCircle size={16} class="shrink-0 text-destructive" aria-hidden="true" />
            <p class="text-sm font-medium text-destructive">{$t("integrations.wizard.added_failed")}</p>
            <Badge size="sm" variant="danger">{$t("integrations.wizard.failed_badge")}</Badge>
          </div>
          <p class="text-sm text-foreground" data-testid="test-result-error-message">
            {$t(trialMessageKey ?? "integrations.wizard.test_err.generic")}
          </p>
          {#if trialDetail}
            <details class="text-xs">
              <summary class="cursor-pointer text-muted-foreground">{$t("integrations.wizard.test_err.raw_details")}</summary>
              <p class="mt-1 break-all font-mono text-caption text-destructive/80">{trialDetail}</p>
            </details>
          {/if}
          {#if removeError}
            <p class="text-xs text-destructive" data-testid="wizard-remove-error">{removeError}</p>
          {/if}
        </div>
      {/if}
    {:else if phase === "refused" && refusal}
      <ConnectionRefusal {refusal} onopen={openRefusingRule} />
    {:else if phase === "failed"}
      <div class="space-y-2 rounded-md border border-destructive/30 bg-destructive/5 px-4 py-3" data-testid="wizard-finalize-error">
        <div class="flex items-center gap-2">
          <XCircle size={16} class="shrink-0 text-destructive" aria-hidden="true" />
          <p class="text-sm font-medium text-destructive">{$t("integrations.wizard.add_failed")}</p>
        </div>
        {#if failureNode}
          <p class="text-sm text-foreground" data-testid="wizard-node-unavailable">
            {$t("integrations.wizard.node_unavailable", { values: { major: MIN_NODE_MAJOR } })}
          </p>
          <a
            href={docsUrl(NODE_HELP_PATH)}
            onclick={handleExternalLinkClick}
            class="text-xs text-primary underline-offset-2 hover:underline"
            data-testid="wizard-node-help">{$t("integrations.wizard.node_unavailable_help")}</a
          >
          {#if failure}
            <details class="text-xs">
              <summary class="cursor-pointer text-muted-foreground">{$t("integrations.wizard.test_err.raw_details")}</summary>
              <p class="mt-1 break-all font-mono text-caption text-muted-foreground">{failure}</p>
            </details>
          {/if}
        {:else if failure}
          <p class="break-all text-xs text-muted-foreground">{failure}</p>
        {/if}
      </div>
    {/if}
  </div>

  <!-- The address and the mailbox carry their own buttons with their form. -->
  {#if !(phase === "connect" && (entry.kind === "address" || entry.kind === "mailbox"))}
    <footer class="mt-auto flex items-center justify-end gap-2 border-t border-border pt-4">
      {#if phase === "connect"}
        <Button variant="ghost" size="sm" onclick={onback} data-testid="wizard-cancel-btn">{$t("common.cancel")}</Button>
        {#if entry.kind === "catalogue" && setup}
          {#if !warningAccepted}
            <Button variant="primary-solid" size="sm" onclick={() => void acceptWarning()} data-testid="wizard-warning-accept">
              {$t("integrations.wizard.confirm_button")}
            </Button>
          {:else}
            <Button
              variant="primary-solid"
              size="sm"
              disabled={!setup.complete}
              onclick={() => void addServer(setup.config(), setup.secrets(), true)}
              data-testid="wizard-auth-continue"
            >
              {$t("integrations.wizard.confirm_button")}
            </Button>
          {/if}
        {:else if entry.kind === "folder"}
          <Button variant="primary-solid" size="sm" onclick={() => void pickFolder()} data-testid="add-flow-folder-pick">
            {#snippet icon()}<Folder size={12} />{/snippet}
            {$t("connections.add_flow.folder_pick")}
          </Button>
        {:else if entry.kind === "native"}
          {@const provider = entry.provider}
          <Button variant="primary-solid" size="sm" onclick={() => onsignin?.(provider)} data-testid="add-flow-signin">
            {#snippet icon()}<LogIn size={12} />{/snippet}
            {$t("connections.add_flow.sign_in")}
          </Button>
        {/if}
      {:else if phase === "who"}
        <Button variant="primary-solid" size="sm" onclick={() => void confirmWho()} data-testid="add-flow-who-continue">
          {$t("common.continue")}
        </Button>
      {:else if phase === "trial"}
        {#if trialOk}
          {#if ontry && connectionId}
            {@const id = connectionId}
            <Button variant="outline" size="sm" onclick={() => ontry(id, name)} data-testid="add-flow-try">
              {#snippet icon()}<Play size={12} />{/snippet}
              {$t("connections.try.action")}
            </Button>
          {/if}
        {:else}
          <Button variant="destructive" size="sm" onclick={() => void removeAdded()} disabled={removing} data-testid="wizard-remove-btn">
            {$t("integrations.wizard.remove")}
          </Button>
          <Button variant="outline" size="sm" onclick={() => void trial()} disabled={removing} data-testid="test-retry-btn">
            {$t("integrations.wizard.test_retry")}
          </Button>
        {/if}
        <Button variant="primary-solid" size="sm" onclick={() => ondone(connectionId)} data-testid="wizard-done-btn">
          {$t("connections.folder.done")}
        </Button>
      {:else if phase === "failed"}
        <Button variant="ghost" size="sm" onclick={onback} data-testid="wizard-cancel-btn">{$t("common.cancel")}</Button>
        {#if serverConfig || setup}
          <Button variant="primary-solid" size="sm" onclick={retryAdd} data-testid="wizard-retry-add-btn">
            {$t("integrations.wizard.test_retry")}
          </Button>
        {:else if entry.kind === "folder"}
          <Button variant="primary-solid" size="sm" onclick={() => void pickFolder()} data-testid="folder-connection-choose-another">
            {#snippet icon()}<Folder size={12} />{/snippet}
            {$t("connections.folder.choose_another")}
          </Button>
        {/if}
      {:else if phase === "refused"}
        <Button variant="primary-solid" size="sm" onclick={onback} data-testid="wizard-done-btn">{$t("common.close")}</Button>
      {/if}
    </footer>
  {/if}
</div>
