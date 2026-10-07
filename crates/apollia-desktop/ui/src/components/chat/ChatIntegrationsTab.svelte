<script module lang="ts">
  import type { ProfileSummaryView } from "$lib/ipc/profile";
  import type { ConversationStatsView } from "$lib/types";

  /**
   * The value of the profile select for a session: its designated profile
   * while that profile exists, otherwise `""`, the current default. A
   * designation of a profile deleted since reads as empty.
   */
  export function profileSelection(
    profileId: string | null | undefined,
    profiles: readonly ProfileSummaryView[],
  ): string {
    if (!profileId) return "";
    return profiles.some((p) => p.id === profileId) ? profileId : "";
  }

  /** What the session stores for a select value: `""` designates no profile. */
  export function profileIdOf(selection: string): string | null {
    return selection === "" ? null : selection;
  }

  /**
   * The profile switch of a session: on unless the session stored it off.
   * Before a conversation exists the switch shows `byDefault`, the position
   * Settings > Integrations gives every new conversation.
   */
  export function profileSwitchOf(
    session: { use_profile?: boolean } | null,
    byDefault = true,
  ): boolean {
    return session ? session.use_profile !== false : byDefault;
  }

  /** What writing the profile switch of a session needs from the runtime. */
  export interface ProfileSwitchEffects {
    update: (sessionId: string, update: { use_profile: boolean }) => Promise<void>;
    stats: (sessionId: string) => Promise<ConversationStatsView>;
  }

  /**
   * Stores the profile switch of a session, then reads its gauge again so the
   * indicator follows the switch. The gauge is `null` when it cannot be read;
   * a refused write throws and nothing is read.
   */
  export async function writeProfileSwitch(
    sessionId: string,
    on: boolean,
    effects: ProfileSwitchEffects,
  ): Promise<ConversationStatsView | null> {
    await effects.update(sessionId, { use_profile: on });
    try {
      return await effects.stats(sessionId);
    } catch {
      return null;
    }
  }
</script>

<script lang="ts">
  /**
   * The Integrations tab of the right panel of the chat: what holds for this
   * conversation alone. Its profile and whether the agent knows it, then what
   * the agent reaches here, each access a switch. Every control writes at
   * once; neither the agent nor the permissions are changed.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { User } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Select } from "$lib/components/ui/select";
  import type { ChatSessionDetail } from "$lib/types";
  import { getConversationStats, updateChatSession } from "$lib/ipc/chat";
  import { getChatSessionSettings, setChatSessionSettings } from "$lib/ipc/chatSettings";
  import { getAgentDefinition } from "$lib/ipc/agentSheet";
  import { listTools } from "$lib/ipc/tools";
  import { listMcpExposable } from "$lib/ipc/mcpServer";
  import { listProfiles } from "$lib/ipc/profile";
  import { useUserMemory, chatConversationStats, currentSession } from "$lib/stores/chat";
  import { shareProfileDefault, hydrateShareProfileDefault } from "$lib/stores/shareProfileDefault";
  import { TOOL_CATALOG } from "$lib/tools/tool-catalog";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import { agentIdOf } from "$lib/chat/sessionSettings";
  import { PROFILE_SWITCH_KEYS, profileSwitchCase } from "$lib/chat/sessionAccesses";
  import {
    createSessionAccesses,
    type SessionAccessesController,
  } from "./integrations/useSessionAccesses.svelte";
  import SessionAccessList from "./integrations/SessionAccessList.svelte";

  interface Props {
    session: ChatSessionDetail | null;
    onupdated: () => void;
    /** The accesses shown; the tab creates its own when left out. */
    controller?: SessionAccessesController;
  }

  let {
    session,
    onupdated,
    controller = createSessionAccesses(
      {
        getSettings: getChatSessionSettings,
        setSettings: setChatSessionSettings,
        getAgent: getAgentDefinition,
        listTools,
        listTargets: listMcpExposable,
      },
      new Map(
        TOOL_CATALOG.map((tool) => [
          tool.id,
          { nameKey: tool.labelOperatorKey, detailKey: tool.descOperatorKey },
        ]),
      ),
    ),
  }: Props = $props();

  let saveError = $state<string | null>(null);
  let profiles = $state<ProfileSummaryView[]>([]);
  let selectedProfile = $state("");

  const defaultProfileName = $derived(profiles.find((p) => p.is_default)?.name ?? null);
  const isClosed = $derived(session?.status === "closed");
  const switchCase = $derived(profileSwitchCase(session));
  const error = $derived(saveError ?? controller.error);

  /**
   * The profile of the conversation is stored as it is chosen: the runtime
   * reads it at the next message. A refused write puts the select back and
   * says why.
   */
  async function handleProfileChoice(value: string): Promise<void> {
    if (!session || isClosed) return;
    const sessionId = session.id;
    const previous = selectedProfile;
    selectedProfile = value;
    saveError = null;
    const profileId = profileIdOf(profileSelection(value, profiles));
    try {
      await updateChatSession(sessionId, { profile_id: profileId });
      currentSession.update((s) => (s && s.id === sessionId ? { ...s, profile_id: profileId } : s));
      onupdated();
    } catch (err: unknown) {
      selectedProfile = previous;
      saveError = err instanceof Error ? err.message : String(err);
    }
  }

  /**
   * The switch acts at once: the runtime reads it at the next message. A
   * refused write puts the switch back and says why.
   */
  async function handleProfileSwitch(on: boolean): Promise<void> {
    if (!session || isClosed) return;
    const sessionId = session.id;
    const previous = $useUserMemory;
    useUserMemory.set(on);
    saveError = null;
    try {
      const stats = await writeProfileSwitch(sessionId, on, {
        update: updateChatSession,
        stats: getConversationStats,
      });
      currentSession.update((s) => (s && s.id === sessionId ? { ...s, use_profile: on } : s));
      if (stats) chatConversationStats.set(stats);
    } catch (err: unknown) {
      useUserMemory.set(previous);
      saveError = err instanceof Error ? err.message : String(err);
    }
  }

  $effect(() => {
    let cancelled = false;
    listProfiles()
      .then((list) => {
        if (!cancelled) profiles = list;
      })
      .catch(() => {
        // Without the list the select offers the default alone.
      });
    return () => {
      cancelled = true;
    };
  });

  // The tab stays mounted across conversations: each one is read as it opens.
  let loadedFor: string | null = null;
  $effect(() => {
    const id = session?.id ?? null;
    const agentId = session ? agentIdOf(session, APOLLIA_AGENT_ID) : APOLLIA_AGENT_ID;
    const key = id ? `${id}|${agentId}` : null;
    if (!session || !id || key === loadedFor) return;
    loadedFor = key;
    selectedProfile = session.profile_id ?? "";
    useUserMemory.set(profileSwitchOf(session));
    saveError = null;
    untrack(() => void controller.load(id, agentId));
  });

  // Before a conversation exists, the switch shows the position a new
  // conversation is born with; an existing conversation keeps its own.
  $effect(() => {
    void hydrateShareProfileDefault();
  });
  $effect(() => {
    if (!session) useUserMemory.set(profileSwitchOf(null, $shareProfileDefault));
  });
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="chat-session-integrations">
  <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4">
    <section class="space-y-2" data-testid="session-profile">
      <div class="flex items-center gap-2">
        <User class="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
        <h3 class="m-0 text-xs font-medium text-foreground">{$t("chat.integrations.profile_title")}</h3>
      </div>
      <div class="space-y-1.5">
        <label for="config-profile-select" class="text-xs text-foreground">
          {$t("chat.config.profile_label")}
        </label>
        <Select
          id="config-profile-select"
          size="sm"
          value={profileSelection(selectedProfile, profiles)}
          onchange={(e: Event) => void handleProfileChoice((e.currentTarget as HTMLSelectElement).value)}
          disabled={isClosed || !session}
          data-testid="config-profile-select"
        >
          <option value="" data-testid="config-profile-option-default">
            {defaultProfileName
              ? $t("chat.config.profile_default", { values: { name: defaultProfileName } })
              : $t("chat.config.profile_default_unnamed")}
          </option>
          {#each profiles as profile (profile.id)}
            <option value={profile.id} data-testid="config-profile-option-{profile.id}">
              {profile.name}
            </option>
          {/each}
        </Select>
        <p class="m-0 text-micro text-muted-foreground">{$t("chat.config.profile_hint")}</p>
      </div>
      <div data-testid="session-profile-switch" data-case={switchCase}>
        <div class="flex items-center justify-between gap-3">
          <span class="text-xs text-foreground">{$t(PROFILE_SWITCH_KEYS[switchCase].label)}</span>
          {#if switchCase === "agent"}
            <Toggle
              size="sm"
              checked={controller.agentReadsProfile}
              disabled={isClosed || controller.loading}
              onchange={(on) => void controller.setProfileAccess(on)}
              aria-label={$t(PROFILE_SWITCH_KEYS.agent.label)}
              data-testid="toggle-profile-access"
            />
          {:else}
            <Toggle
              size="sm"
              checked={$useUserMemory}
              disabled={isClosed}
              onchange={handleProfileSwitch}
              aria-label={$t(PROFILE_SWITCH_KEYS.assistant.label)}
              data-testid="toggle-user-memory"
            />
          {/if}
        </div>
        <p class="m-0 mt-1 text-micro leading-tight text-muted-foreground">
          {$t(PROFILE_SWITCH_KEYS[switchCase].help)}
        </p>
      </div>
    </section>

    <div class="border-t border-border pt-4">
      {#if session}
        <SessionAccessList
          rows={controller.rows}
          loading={controller.loading}
          disabled={isClosed}
          onswitch={(row, on) => void controller.switchAccess(row, on)}
        />
      {:else}
        <p class="m-0 text-caption text-muted-foreground">
          {$t("chat.integrations.no_conversation")}
        </p>
      {/if}
    </div>

    {#if error}
      <div class="rounded-lg border border-destructive/30 bg-destructive/5 px-3.5 py-2.5 animate-fade-in">
        <p class="m-0 text-xs text-destructive" data-testid="config-error">{error}</p>
      </div>
    {/if}
  </div>
</div>
