<script lang="ts">
  /**
   * JournalVerdict - what "Verify integrity" found, in plain words: the
   * journal is intact since its beginning or its last purge, or an alteration
   * is detected at an entry. Every verdict carries the limit of the check: a
   * removal at the end is only caught with an anchor exported beforehand.
   */
  import { locale, t } from "svelte-i18n";
  import { AlertTriangle, CheckCircle2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { cn } from "$lib/utils";
  import { isSound, type Verdict, type VerifiedSince } from "./journalVerify";

  interface Props {
    verdict: Verdict;
    checkedAt: Date;
    onExport: () => void;
    onClose: () => void;
  }

  let { verdict, checkedAt, onExport, onClose }: Props = $props();

  const sound = $derived(isSound(verdict));

  function day(value: Date): string {
    return value.toLocaleDateString($locale ?? "en", { day: "numeric", month: "long", year: "numeric" });
  }

  function hour(value: Date): string {
    return value.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
  }

  /** The day the verified part begins, `null` when it is the origin or unreadable. */
  function purgeDay(since: VerifiedSince): string | null {
    if (since.from !== "purge") return null;
    const date = new Date(since.firstEntryAt);
    return Number.isNaN(date.getTime()) ? null : day(date);
  }

  const title = $derived.by(() => {
    if (verdict.kind === "intact") return $t("journal.verify.intact_title");
    if (verdict.kind === "empty") return $t("journal.verify.empty_title");
    if (verdict.kind === "end_mismatch") return $t("journal.verify.end_title");
    return $t("journal.verify.altered_title", { values: { entry: verdict.entry } });
  });

  const body = $derived.by(() => {
    if (verdict.kind === "empty") return $t("journal.verify.empty_body");
    if (verdict.kind === "intact") {
      const date = purgeDay(verdict.since);
      return date === null
        ? $t("journal.verify.intact_origin", { values: { count: verdict.entries } })
        : $t("journal.verify.intact_purge", { values: { count: verdict.entries, date } });
    }
    if (verdict.kind === "end_mismatch") {
      return $t("journal.verify.end_body", { values: { count: verdict.entries } });
    }
    const parts = [$t("journal.verify.altered_body", { values: { entry: verdict.entry } })];
    if (verdict.intactBefore !== null) {
      parts.push($t("journal.verify.altered_before", { values: { count: verdict.intactBefore } }));
    }
    return parts.join(" ");
  });

  const purgeNote = $derived.by(() => {
    if (verdict.kind === "empty" || verdict.kind === "intact") return null;
    const date = purgeDay(verdict.since);
    return date === null ? null : $t("journal.verify.since_purge", { values: { date } });
  });
</script>

<section
  class={cn(
    "flex flex-wrap items-start gap-3 rounded-lg border bg-card px-4 py-3",
    sound ? "border-success" : "border-destructive",
  )}
  role="status"
  data-testid="journal-verdict"
  data-verdict={verdict.kind}
>
  <span class={cn("mt-0.5 shrink-0", sound ? "text-success" : "text-destructive")} aria-hidden="true">
    {#if sound}
      <CheckCircle2 size={16} strokeWidth={1.75} />
    {:else}
      <AlertTriangle size={16} strokeWidth={1.75} />
    {/if}
  </span>

  <div class="min-w-0 flex-1 basis-80 space-y-1.5">
    <h3 class={cn("m-0 text-body-sm font-medium", !sound && "text-destructive")} data-testid="journal-verdict-title">
      {title}
    </h3>
    <p class="m-0 text-body-sm" data-testid="journal-verdict-body">
      {body}
      {#if purgeNote !== null}{purgeNote}{/if}
    </p>
    {#if verdict.kind !== "empty"}
      <p class="m-0 text-caption text-muted-foreground" data-testid="journal-verdict-limit">
        {$t("journal.verify.limit")}
      </p>
    {/if}
    <p class="m-0 text-caption text-muted-foreground">
      {$t("journal.verify.checked_at", { values: { date: day(checkedAt), time: hour(checkedAt) } })}
    </p>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    {#if !sound}
      <Button variant="outline" size="sm" onclick={onExport} data-testid="journal-verdict-export">
        {$t("journal.verify.export_as_is")}
      </Button>
    {/if}
    <Button variant="ghost" size="sm" onclick={onClose} data-testid="journal-verdict-close">
      {$t("journal.verify.close")}
    </Button>
  </div>
</section>
