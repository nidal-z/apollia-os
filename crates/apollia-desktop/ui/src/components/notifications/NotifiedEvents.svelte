<script lang="ts">
  /**
   * The notified events of the application: one line per event, its sentence
   * and its switch, saved at once. Only the events switched on here can
   * trigger a notification; a channel may then keep a part of them.
   */
  import { t } from "svelte-i18n";
  import { BellRing } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import SettingsSection from "../settings/SettingsSection.svelte";
  import {
    NOTIFICATION_EVENT_TYPES,
    eventLabelKey,
    eventDescriptionKey,
  } from "$lib/notifications/event-labels";

  interface Props {
    /** The events switched on, `null` while they load. */
    events: string[] | null;
    /** The event whose switch is being saved, if any. */
    saving: string | null;
    ontoggle: (event: string) => void;
  }

  let { events, saving, ontoggle }: Props = $props();
</script>

<SettingsSection
  title={$t("settings.notifications.events_title")}
  description={$t("settings.notifications.events_desc")}
  bodyLayout="flush"
  data-testid="global-events-section"
>
  {#snippet icon()}<BellRing size={15} strokeWidth={1.75} />{/snippet}
  {#if events === null}
    <div class="space-y-2 px-4 pb-4">
      <Skeleton variant="text" />
      <Skeleton variant="text" />
    </div>
  {:else}
    <ul>
      {#each NOTIFICATION_EVENT_TYPES as event (event)}
        {@const on = events.includes(event)}
        <li class="flex items-center gap-3 border-b border-border px-4 py-2.5 last:border-b-0">
          <div class="min-w-0 flex-1">
            <p class="text-body-sm text-foreground">{$t(eventLabelKey(event))}</p>
            <p class="text-caption text-muted-foreground">{$t(eventDescriptionKey(event))}</p>
          </div>
          <Toggle
            checked={on}
            onchange={() => ontoggle(event)}
            loading={saving === event}
            size="sm"
            aria-label={$t(eventLabelKey(event))}
            data-testid="global-event-{event}"
          />
        </li>
      {/each}
    </ul>
  {/if}
</SettingsSection>
