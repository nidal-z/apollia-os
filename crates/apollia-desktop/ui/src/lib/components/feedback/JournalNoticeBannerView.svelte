<script lang="ts">
  /**
   * JournalNoticeBannerView - what the window says while the journal is
   * degraded and nothing is refused, with no store and no IPC, so it renders
   * under test.
   *
   * One warning line per notice, full width under the top bar: the journal
   * opened without its signing key, or the daily export the organisation
   * policy asks for could not be written.
   */
  import { t } from "svelte-i18n";
  import { Banner } from "$lib/components/ui/banner";
  import type { JournalNotice } from "$lib/ipc/governance";

  let { notices }: { notices: JournalNotice[] } = $props();
</script>

{#each notices as notice (notice.notice)}
  <Banner variant="warning" data-notice={notice.notice}>
    {#if notice.notice === "unsigned"}
      {$t("governance.journal_notice.unsigned")}
    {:else}
      {$t("governance.journal_notice.daily_export_failed", {
        values: { directory: notice.directory },
      })}
    {/if}
  </Banner>
{/each}
