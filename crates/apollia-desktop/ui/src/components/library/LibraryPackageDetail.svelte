<script lang="ts">
  /**
   * An open package of the Library: who published it and whether it is
   * signed, what it holds by kind, and the two gestures on it, update and
   * uninstall. The packages provided with Apollia open the same way, without
   * those gestures: they update with the application and are never uninstalled.
   */
  import { t, locale } from "svelte-i18n";
  import { Bot, ChevronRight, Package, Plug, ShieldAlert, ShieldCheck, Workflow, Wrench } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { shortFingerprint } from "../agents/sheet/agentExport";
  import {
    emptyKinds,
    enumerate,
    installedOn,
    type LibraryDetail,
  } from "$lib/library/libraryState";
  import LibraryContentGroup, { type ContentLine } from "./LibraryContentGroup.svelte";

  interface Props {
    detail: LibraryDetail;
    onback: () => void;
    onupdate: () => void;
    onuninstall: () => void;
    onopenagent: (id: string) => void;
    onopenworkflow: (id: string) => void;
  }

  let { detail, onback, onupdate, onuninstall, onopenagent, onopenworkflow }: Props = $props();

  const name = $derived(detail.provided ? $t("library.provided.name") : detail.key);
  const installed = $derived(installedOn(detail.installedAt, $locale ?? "en"));
  const origin = $derived(
    detail.provided
      ? $t("library.provided.origin")
      : detail.publisher || $t("library.publisher_unknown"),
  );

  const agents = $derived<ContentLine[]>(detail.content.agents.map((a) => ({ ...a })));
  const workflows = $derived<ContentLine[]>(detail.content.workflows.map((w) => ({ ...w })));
  const tools = $derived<ContentLine[]>(
    detail.content.tools.map((tool) => ({
      id: tool.name,
      name: tool.name,
      description: tool.description,
      tag: tool.family ? $t(`packages.preview.access.family.${tool.family}.title`) : undefined,
    })),
  );
  const connections = $derived<ContentLine[]>([
    ...detail.content.connections_brought.map((connection) => ({
      id: `brought:${connection}`,
      name: connection,
      description: $t("library.detail.connection_brought"),
    })),
    ...detail.content.connections_required.map((connection) => ({
      id: `required:${connection.type}`,
      name: connection.type,
      description: connection.reason,
      tag: $t("library.detail.connection_required"),
    })),
  ]);
  const missing = $derived(
    enumerate(
      emptyKinds(detail.content).map((kind) => $t(`library.kind.${kind}`)),
      $t("library.detail.none_joiner"),
    ),
  );
</script>

<div class="flex flex-col gap-6 px-8 py-8" data-testid="library-detail" data-package={detail.key}>
  <nav class="flex items-center gap-1 text-caption text-muted-foreground" aria-label={$t("a11y.breadcrumb")}>
    <button
      type="button"
      class="rounded px-1 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      onclick={onback}
      data-testid="library-detail-back"
    >
      {$t("library.title")}
    </button>
    <ChevronRight size={12} />
    <span class="text-foreground">{name}</span>
  </nav>

  <header class="flex flex-wrap items-start gap-4">
    <span class="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
      <Package size={22} />
    </span>
    <div class="min-w-0 flex-1">
      <h1 class="text-heading-lg font-semibold text-foreground" data-testid="library-detail-title">{name}</h1>
      <p class="text-caption text-muted-foreground" data-testid="library-detail-meta">
        {origin} · {$t("packages.preview.version", { values: { version: detail.version } })}
        {#if installed}· {$t("library.detail.installed_on", { values: { date: installed } })}{/if}
      </p>
      {#if detail.description}
        <p class="mt-1 text-body-sm text-foreground">{detail.description}</p>
      {/if}
    </div>
    {#if detail.provided}
      <Badge variant="neutral" outline data-testid="library-detail-provided">
        {$t("library.provided.badge")}
      </Badge>
    {:else}
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" onclick={onupdate} data-testid="library-update-button">
          {$t("library.detail.update")}
        </Button>
        <Button variant="outline" size="sm" onclick={onuninstall} data-testid="library-uninstall-button">
          {$t("library.detail.uninstall")}
        </Button>
      </div>
    {/if}
  </header>

  <section
    class="flex items-start gap-2.5 rounded-lg border border-border bg-card px-4 py-3"
    data-testid="library-detail-signature"
    data-state={detail.provided ? "provided" : detail.signed ? "signed" : "unsigned"}
  >
    {#if detail.signed}
      <ShieldCheck size={18} class="mt-0.5 shrink-0 text-success-a11y" />
    {:else}
      <ShieldAlert size={18} class="mt-0.5 shrink-0 text-warning-a11y" />
    {/if}
    <div class="min-w-0 flex-1">
      {#if detail.provided}
        <p class="text-body-sm font-medium text-foreground">{$t("library.provided.signature_title")}</p>
        <p class="text-caption text-muted-foreground">{$t("library.provided.signature_body")}</p>
      {:else if detail.signed}
        <p class="text-body-sm font-medium text-foreground">
          {$t("library.detail.signed_title", { values: { publisher: origin } })}
        </p>
        <p class="text-caption text-muted-foreground">
          {detail.trusted ? $t("library.detail.signed_trusted") : $t("library.detail.signed_untrusted")}
        </p>
        {#if detail.keyFingerprint}
          <p class="mt-1 font-mono text-code-sm text-muted-foreground" data-testid="library-detail-fingerprint">
            {$t("packages.preview.publisher.fingerprint")}
            {shortFingerprint(detail.keyFingerprint)}
          </p>
        {/if}
      {:else}
        <p class="text-body-sm font-medium text-foreground">{$t("library.detail.unsigned_title")}</p>
        <p class="text-caption text-muted-foreground">{$t("library.detail.unsigned_body")}</p>
      {/if}
    </div>
  </section>

  <section class="flex flex-col gap-3">
    <h2 class="text-heading-sm font-semibold text-foreground">{$t("library.detail.content_title")}</h2>
    <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
      <div class="flex flex-col gap-5">
        <LibraryContentGroup kind="agent" title={$t("library.detail.agents")} icon={Bot} lines={agents} onopen={onopenagent} />
        <LibraryContentGroup
          kind="workflow"
          title={$t("library.detail.workflows")}
          icon={Workflow}
          lines={workflows}
          onopen={onopenworkflow}
        />
      </div>
      <div class="flex flex-col gap-5">
        <LibraryContentGroup kind="tool" title={$t("library.detail.tools")} icon={Wrench} lines={tools} />
        <LibraryContentGroup kind="connection" title={$t("library.detail.connections")} icon={Plug} lines={connections} />
      </div>
    </div>
    {#if missing}
      <p class="text-caption text-muted-foreground" data-testid="library-detail-missing">
        {$t("library.detail.none", { values: { kinds: missing } })}
      </p>
    {/if}
  </section>

  <p class="text-caption text-muted-foreground" data-testid="library-detail-update-note">
    {detail.provided ? $t("library.provided.update_note") : $t("library.detail.update_note")}
  </p>
</div>
