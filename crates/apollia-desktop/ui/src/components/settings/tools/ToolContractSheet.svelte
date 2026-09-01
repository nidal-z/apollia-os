<script lang="ts">
  /**
   * ToolContractSheet - the contract of a native tool, read-only.
   *
   * The governance page shows how a tool is configured; this panel shows what
   * the tool actually is: kind, version, required permissions, input and output
   * schemas, and the credentials registered against it. The schemas render as
   * indented field rows (name, type, required, default, allowed values) rather
   * than as a block of JSON, with the raw document one click away for the cases
   * the walker cannot flatten.
   *
   * Credential values never travel to the frontend: only key names and the
   * creation date are displayed. Last use is deliberately absent, see
   * `$lib/ipc/governance`.
   */
  import { t, locale } from "svelte-i18n";
  import {
    Braces,
    KeyRound,
    ShieldCheck,
    ChevronDown,
    ChevronRight,
  } from "lucide-svelte";
  import { Sheet, SheetHeader, SheetContent } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner, EmptyState } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import { cn, formatRelativeTime } from "$lib/utils";
  import { describeTool, toolKindTag, type ToolDescriptor } from "$lib/ipc/tools";
  import { governanceListCredentials, type CredentialEntry } from "$lib/ipc/governance";
  import {
    flattenSchema,
    hasSchema,
    prettyJson,
    type SchemaField,
  } from "./schemaSummary";

  interface Props {
    open: boolean;
    /** Canonical tool identifier, `null` when no tool is selected. */
    toolName: string | null;
    /** Humanized tool name for the header; falls back to the identifier. */
    title?: string;
    onclose: () => void;
  }

  let { open, toolName, title, onclose }: Props = $props();

  let descriptor = $state<ToolDescriptor | null>(null);
  let credentials = $state<CredentialEntry[]>([]);
  let loading = $state(false);
  let failure = $state<string | null>(null);
  let missing = $state(false);
  let loadedFor = $state<string | null>(null);
  let rawInput = $state(false);
  let rawOutput = $state(false);

  const displayName = $derived(title || toolName || "");

  $effect(() => {
    if (!open || !toolName) return;
    if (loadedFor === toolName) return;
    const requested = toolName;
    loadedFor = requested;
    loading = true;
    failure = null;
    missing = false;
    descriptor = null;
    credentials = [];
    void Promise.all([describeTool(requested), governanceListCredentials(requested)])
      .then(([desc, creds]) => {
        if (loadedFor !== requested) return;
        descriptor = desc;
        missing = desc === null;
        credentials = creds;
      })
      .catch((err: unknown) => {
        if (loadedFor !== requested) return;
        failure = reportError(err, { surface: "inline" }).friendly_message;
      })
      .finally(() => {
        if (loadedFor === requested) loading = false;
      });
  });

  $effect(() => {
    if (open) return;
    loadedFor = null;
    rawInput = false;
    rawOutput = false;
  });

  /**
   * True until the descriptor of the currently selected tool has landed.
   * `loadedFor !== toolName` covers the first render, which happens before the
   * loading effect runs and would otherwise flash the empty state.
   */
  const pending = $derived(loading || loadedFor !== toolName);

  const inputFields = $derived<SchemaField[]>(
    flattenSchema(descriptor?.input_schema),
  );
  const outputFields = $derived<SchemaField[]>(
    flattenSchema(descriptor?.output_schema),
  );
  const hasInput = $derived(hasSchema(descriptor?.input_schema));
  const hasOutput = $derived(hasSchema(descriptor?.output_schema));

  /**
   * Label key per `ToolKind` discriminant. The backend serializes the enum as
   * a tagged object, so the tag is read with `toolKindTag` rather than assumed
   * to be a bare string; an unknown tag falls back to its raw value instead of
   * hiding the badge.
   */
  const KIND_LABEL_KEY: Record<string, string> = {
    native: "settings.tool_contract.kind_native",
    mcp_server: "settings.tool_contract.kind_mcp_server",
    custom: "settings.tool_contract.kind_custom",
  };

  const kindTag = $derived(toolKindTag(descriptor?.kind));
  const kindLabel = $derived.by(() => {
    if (kindTag === null) return null;
    const key = KIND_LABEL_KEY[kindTag];
    return key ? $t(key) : kindTag;
  });

  function reload(): void {
    loadedFor = null;
  }

  /** Left inset of a nested field row, one step per nesting level. */
  const INDENT: Record<number, string> = {
    0: "pl-0",
    1: "pl-4",
    2: "pl-8",
  };

  function indentClass(depth: number): string {
    return INDENT[depth] ?? "pl-8";
  }
</script>

<Sheet
  {open}
  {onclose}
  width="lg"
  ariaLabel={$t("settings.tool_contract.aria", {
    values: { name: displayName },
  })}
>
  <SheetHeader
    title={$t("settings.tool_contract.title", { values: { name: displayName } })}
    {onclose}
    closeLabel={$t("common.close")}
    class="px-5 py-4 items-center"
  >
    {#snippet titleSlot()}
      <div class="space-y-0.5">
        <h2 class="text-lg font-semibold">
          {$t("settings.tool_contract.title", { values: { name: displayName } })}
        </h2>
        <p class="font-mono text-caption text-muted-foreground">
          {toolName ?? ""}
        </p>
      </div>
    {/snippet}
  </SheetHeader>

  <SheetContent data-testid="tool-contract-sheet">
    {#if pending}
      <p class="text-body-sm text-muted-foreground" data-testid="tool-contract-loading">
        {$t("common.loading")}
      </p>
    {:else if failure}
      <ErrorBanner
        message={failure}
        onretry={reload}
        retryLabel={$t("settings.tool_contract.retry")}
        data-testid="tool-contract-error"
      />
    {:else if missing || !descriptor}
      <div data-testid="tool-contract-missing">
        <EmptyState
          title={$t("settings.tool_contract.missing_title")}
          desc={$t("settings.tool_contract.missing_desc")}
          tone="neutral"
        >
          {#snippet icon()}<Braces size={22} strokeWidth={1.75} aria-hidden="true" />{/snippet}
        </EmptyState>
      </div>
    {:else}
      <section class="space-y-2.5" data-testid="tool-contract-identity">
        <div class="flex flex-wrap items-center gap-1.5">
          {#if kindLabel}
            <Badge variant="neutral" size="sm" data-testid="tool-contract-kind">
              {kindLabel}
            </Badge>
          {/if}
          {#if descriptor.version}
            <Badge variant="outline" size="sm" data-testid="tool-contract-version">
              v{descriptor.version}
            </Badge>
          {/if}
        </div>
        {#if descriptor.description}
          <p class="max-w-prose text-body-sm text-muted-foreground">
            {descriptor.description}
          </p>
        {/if}
      </section>

      <!--
        The section stays hidden when the descriptor carries no permission:
        the runtime descriptor has no permission field today, so an empty list
        means "not reported", never "none required".
      -->
      {#if descriptor.permissions.length}
        <section class="space-y-1.5" data-testid="tool-contract-permissions">
          <h3 class="flex items-center gap-1.5 text-overline font-semibold uppercase text-muted-foreground">
            <ShieldCheck size={13} strokeWidth={1.9} aria-hidden="true" />
            {$t("settings.tool_contract.permissions")}
          </h3>
          <div class="flex flex-wrap gap-1.5">
            {#each descriptor.permissions as permission (permission)}
              <Badge
                variant="warning"
                size="sm"
                outline
                data-testid="tool-contract-permission-{permission}"
              >
                {permission}
              </Badge>
            {/each}
          </div>
        </section>
      {/if}

      <section class="space-y-1.5" data-testid="tool-contract-input">
        <div class="flex items-center gap-2">
          <h3 class="flex items-center gap-1.5 text-overline font-semibold uppercase text-muted-foreground">
            <Braces size={13} strokeWidth={1.9} aria-hidden="true" />
            {$t("settings.tool_contract.input_schema")}
          </h3>
          {#if hasInput}
            <Button
              variant="ghost"
              size="sm"
              class="ml-auto"
              onclick={() => (rawInput = !rawInput)}
              data-testid="tool-contract-input-raw-toggle"
            >
              {#snippet icon()}
                {#if rawInput}
                  <ChevronDown size={13} strokeWidth={2} aria-hidden="true" />
                {:else}
                  <ChevronRight size={13} strokeWidth={2} aria-hidden="true" />
                {/if}
              {/snippet}
              {$t("settings.tool_contract.raw_json")}
            </Button>
          {/if}
        </div>

        {#if !hasInput}
          <p class="text-caption text-muted-foreground" data-testid="tool-contract-input-none">
            {$t("settings.tool_contract.no_input")}
          </p>
        {:else}
          {#if inputFields.length}
            <ul class="flex flex-col divide-y divide-border/50 rounded-lg border border-border bg-card">
              {#each inputFields as field (field.path)}
                <li
                  class={cn("px-3 py-2", indentClass(field.depth))}
                  data-testid="tool-contract-field-{field.path}"
                >
                  <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                    <span class="font-mono text-body-sm text-foreground">{field.name}</span>
                    <span class="font-mono text-overline text-primary">{field.type}</span>
                    {#if field.required}
                      <span class="rounded-full leading-none bg-warning/15 px-1.5 text-overline font-semibold text-warning-a11y">
                        {$t("settings.tool_contract.required")}
                      </span>
                    {:else}
                      <span class="text-overline text-muted-foreground/80">
                        {$t("settings.tool_contract.optional")}
                      </span>
                    {/if}
                    {#if field.defaultValue !== null}
                      <span class="text-overline text-muted-foreground">
                        {$t("settings.tool_contract.default_value", {
                          values: { value: field.defaultValue },
                        })}
                      </span>
                    {/if}
                  </div>
                  {#if field.description}
                    <p class="mt-0.5 max-w-prose text-caption text-muted-foreground">
                      {field.description}
                    </p>
                  {/if}
                  {#if field.enumValues}
                    <p class="mt-0.5 text-caption text-muted-foreground">
                      {$t("settings.tool_contract.allowed_values", {
                        values: { values: field.enumValues.join(", ") },
                      })}
                    </p>
                  {/if}
                </li>
              {/each}
            </ul>
          {:else}
            <p class="text-caption text-muted-foreground" data-testid="tool-contract-input-opaque">
              {$t("settings.tool_contract.schema_not_flattenable")}
            </p>
          {/if}

          {#if rawInput || inputFields.length === 0}
            <pre
              class="m-0 max-h-80 overflow-auto whitespace-pre rounded-md border border-border/60 bg-surface-1 px-3 py-2 font-mono text-caption leading-relaxed text-foreground"
              data-testid="tool-contract-input-raw">{prettyJson(descriptor.input_schema)}</pre>
          {/if}
        {/if}
      </section>

      {#if hasOutput}
        <section class="space-y-1.5" data-testid="tool-contract-output">
          <div class="flex items-center gap-2">
            <h3 class="flex items-center gap-1.5 text-overline font-semibold uppercase text-muted-foreground">
              <Braces size={13} strokeWidth={1.9} aria-hidden="true" />
              {$t("settings.tool_contract.output_schema")}
            </h3>
            <Button
              variant="ghost"
              size="sm"
              class="ml-auto"
              onclick={() => (rawOutput = !rawOutput)}
              data-testid="tool-contract-output-raw-toggle"
            >
              {#snippet icon()}
                {#if rawOutput}
                  <ChevronDown size={13} strokeWidth={2} aria-hidden="true" />
                {:else}
                  <ChevronRight size={13} strokeWidth={2} aria-hidden="true" />
                {/if}
              {/snippet}
              {$t("settings.tool_contract.raw_json")}
            </Button>
          </div>

          {#if outputFields.length}
            <ul class="flex flex-col divide-y divide-border/50 rounded-lg border border-border bg-card">
              {#each outputFields as field (field.path)}
                <li
                  class={cn("px-3 py-2", indentClass(field.depth))}
                  data-testid="tool-contract-output-field-{field.path}"
                >
                  <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                    <span class="font-mono text-body-sm text-foreground">{field.name}</span>
                    <span class="font-mono text-overline text-primary">{field.type}</span>
                  </div>
                  {#if field.description}
                    <p class="mt-0.5 max-w-prose text-caption text-muted-foreground">
                      {field.description}
                    </p>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}

          {#if rawOutput || outputFields.length === 0}
            <pre
              class="m-0 max-h-80 overflow-auto whitespace-pre rounded-md border border-border/60 bg-surface-1 px-3 py-2 font-mono text-caption leading-relaxed text-foreground"
              data-testid="tool-contract-output-raw">{prettyJson(descriptor.output_schema)}</pre>
          {/if}
        </section>
      {/if}

      <section class="space-y-1.5" data-testid="tool-contract-credentials">
        <h3 class="flex items-center gap-1.5 text-overline font-semibold uppercase text-muted-foreground">
          <KeyRound size={13} strokeWidth={1.9} aria-hidden="true" />
          {$t("settings.tool_credentials.heading")}
        </h3>
        {#if credentials.length}
          <ul class="flex flex-col divide-y divide-border/50 rounded-lg border border-border bg-card">
            {#each credentials as credential (credential.key_name)}
              <li
                class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 px-3 py-2"
                data-testid="tool-contract-credential-{credential.key_name}"
              >
                <span class="font-mono text-body-sm text-foreground">{credential.key_name}</span>
                <span class="text-overline text-muted-foreground">
                  {$t("settings.tool_credentials.created", {
                    values: { date: formatRelativeTime(credential.created_at, $locale ?? "en") },
                  })}
                </span>
              </li>
            {/each}
          </ul>
          <p class="text-caption text-muted-foreground">
            {$t("settings.tool_credentials.values_hidden")}
          </p>
        {:else}
          <p class="text-caption text-muted-foreground" data-testid="tool-contract-credentials-none">
            {$t("settings.tool_credentials.none_for_tool")}
          </p>
        {/if}
      </section>
    {/if}
  </SheetContent>
</Sheet>
