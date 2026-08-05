<script lang="ts">
  /**
   * CredentialInventory - every credential registered against a native tool.
   *
   * The tool drawer can set, test and delete a credential but never showed the
   * operator what is already stored. This section closes that gap: one row per
   * `(tool, key)` pair with its creation date.
   *
   * No last-use column: the backing column is only written by the manual key
   * test, for one hardcoded pair, so it would read "never used" on every row
   * forever and a real agent read would not change it.
   *
   * No value, and no fragment of a value, is displayed. The backend returns
   * metadata only, and `$lib/ipc/governance` narrows the payload a second time.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { KeyRound, RotateCw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import { formatRelativeTime } from "$lib/utils";
  import { listCredentials, type CredentialEntry } from "$lib/ipc/governance";

  interface Props {
    /** Resolves a tool identifier to its humanized display name. */
    labelFor: (name: string) => string;
  }

  let { labelFor }: Props = $props();

  let entries = $state<CredentialEntry[]>([]);
  let loading = $state(false);
  let failure = $state<string | null>(null);
  let initialized = $state(false);

  async function load(): Promise<void> {
    loading = true;
    failure = null;
    try {
      const rows = await listCredentials();
      entries = [...rows].sort(
        (a, b) =>
          a.tool_name.localeCompare(b.tool_name) ||
          a.key_name.localeCompare(b.key_name),
      );
    } catch (err) {
      failure = reportError(err, { surface: "inline" }).friendly_message;
    } finally {
      loading = false;
      initialized = true;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<section class="flex flex-col gap-1.5" data-testid="credentials-section">
  <div class="flex items-center gap-2 px-0.5">
    <h4 class="flex items-center gap-1.5 text-overline font-semibold uppercase text-muted-foreground">
      <KeyRound size={13} strokeWidth={1.9} aria-hidden="true" />
      {$t("settings.tool_credentials.heading")}
    </h4>
    <span
      class="rounded-full bg-muted/60 px-1.5 text-overline font-semibold text-muted-foreground/80"
      data-testid="credentials-count"
    >
      {entries.length}
    </span>
    <span class="h-px flex-1 bg-gradient-to-r from-border to-transparent" aria-hidden="true"></span>
    <Button
      variant="ghost"
      size="sm"
      onclick={() => void load()}
      loading={loading && initialized}
      data-testid="credentials-reload"
    >
      {#snippet icon()}<RotateCw size={13} strokeWidth={1.9} aria-hidden="true" />{/snippet}
      {$t("settings.tool_credentials.reload")}
    </Button>
  </div>

  {#if failure}
    <ErrorBanner
      message={failure}
      onretry={() => void load()}
      retryLabel={$t("settings.tool_credentials.reload")}
      loading={loading}
      data-testid="credentials-error"
    />
  {:else if !initialized && loading}
    <p class="px-0.5 text-caption text-muted-foreground" data-testid="credentials-loading">
      {$t("common.loading")}
    </p>
  {:else if entries.length === 0}
    <div
      class="rounded-lg border border-dashed border-border bg-card/50 px-3.5 py-3 text-caption text-muted-foreground"
      data-testid="credentials-empty"
    >
      {$t("settings.tool_credentials.empty")}
    </div>
  {:else}
    <ul
      class="flex flex-col divide-y divide-border/50 overflow-hidden rounded-lg border border-border bg-card"
      data-testid="credentials-list"
    >
      {#each entries as entry (`${entry.tool_name}::${entry.key_name}`)}
        <li
          class="flex flex-wrap items-baseline gap-x-2.5 gap-y-1 px-3.5 py-2.5"
          data-testid="credential-row-{entry.tool_name}-{entry.key_name}"
        >
          <span class="text-body-sm font-medium text-foreground">{labelFor(entry.tool_name)}</span>
          <span class="font-mono text-overline tracking-tight text-muted-foreground/80">
            {entry.key_name}
          </span>
          <span class="ml-auto text-overline text-muted-foreground">
            {$t("settings.tool_credentials.created", {
              values: { date: formatRelativeTime(entry.created_at) },
            })}
          </span>
        </li>
      {/each}
    </ul>
    <p class="px-0.5 text-caption text-muted-foreground">
      {$t("settings.tool_credentials.values_hidden")}
    </p>
  {/if}
</section>
