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
  import { t } from "svelte-i18n";
  import { Save, Plug, TriangleAlert, Brain, ChevronDown } from "lucide-svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import type { ChatSessionDetail, UpdateSessionRequest } from "$lib/types";
  import { getConversationStats, updateChatSession } from "$lib/ipc/chat";
  import { TOOL_GROUPS, TOOL_CATALOG, getGroupState, toggleGroup } from "$lib/tools/tool-catalog";
  import { isSensitiveFamily } from "$lib/tools/action-family";
  import { useUserMemory, chatConversationStats, currentSession } from "$lib/stores/chat";
  import { shareProfileDefault, hydrateShareProfileDefault } from "$lib/stores/shareProfileDefault";
  import { developerMode } from "$lib/stores/developerMode";
  import { Card } from "$lib/components/ui/card";
  import { Select } from "$lib/components/ui/select";
  import { listProfiles } from "$lib/ipc/profile";

  interface Props {
    session: ChatSessionDetail | null;
    onupdated: () => void;
    /** Optional close hook - only invoked when the parent wants the panel dismissed after save. */
    onclose?: () => void;
    /** Show the header bar. Drawers embedded in tabs set this to false. */
    showHeader?: boolean;
    /** When true, the parent controls scroll; the body becomes non-scrollable. */
    embedded?: boolean;
  }

  let {
    session,
    onupdated,
    onclose,
    showHeader = true,
    embedded = false,
  }: Props = $props();

  let enabledTools = $state(new Set<string>());
  let collapsedGroups = $state(new Set<string>());
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let profiles = $state<ProfileSummaryView[]>([]);
  let profilesLoaded = $state(false);
  let selectedProfile = $state("");

  const defaultProfileName = $derived(profiles.find((p) => p.is_default)?.name ?? null);

  const isClosed = $derived(session?.status === "closed");
  const isProcessing = $derived(session?.status === "processing");
  const isReadOnly = $derived(isClosed || isProcessing);

  const selectedTools = $derived(Array.from(enabledTools));

  const memoryToggleLabel = $derived(
    $developerMode
      ? $t("chat.inject_user_memory")
      : $t("chat.use_preferences"),
  );

  function syncFromSession(): void {
    if (!session) return;
    enabledTools = new Set(session.available_tools ?? []);
    selectedProfile = session.profile_id ?? "";
    useUserMemory.set(profileSwitchOf(session));
    saveError = null;
  }

  async function handleSave(): Promise<void> {
    if (!session || isReadOnly) return;
    saving = true;
    saveError = null;
    try {
      const update: UpdateSessionRequest = {
        tools: selectedTools,
      };
      if (profilesLoaded) {
        update.profile_id = profileIdOf(profileSelection(selectedProfile, profiles));
      }
      await updateChatSession(session.id, update);
      onupdated();
      onclose?.();
    } catch (err: unknown) {
      saveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  /**
   * The switch acts at once, without the Save button: the runtime reads it at
   * the next message. A refused write puts the switch back and says why.
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

  const isOperator = $derived(!$developerMode);

  $effect(() => {
    let cancelled = false;
    listProfiles()
      .then((list) => {
        if (cancelled) return;
        profiles = list;
        profilesLoaded = true;
      })
      .catch(() => {
        // Without the list the select offers the default alone and a save
        // leaves the designation untouched.
      });
    return () => {
      cancelled = true;
    };
  });

  // Re-sync whenever the session identity changes (drawer stays mounted across sessions).
  let lastSessionId = $state<string | null>(null);
  $effect(() => {
    if (session && session.id !== lastSessionId) {
      lastSessionId = session.id;
      syncFromSession();
    }
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

<div class="flex h-full flex-col" data-testid="chat-session-integrations">
  {#if showHeader}
    <div class="px-5 py-4 border-b glass-border-subtle">
      <div class="flex items-center gap-2.5">
        <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10">
          <Plug class="h-4 w-4 text-primary" />
        </div>
        <h3 class="text-body-sm font-medium">{$t("chat.session_panel.integrations")}</h3>
      </div>
    </div>
  {/if}

  <div class="flex-1 {embedded ? '' : 'overflow-y-auto'} px-5 py-5 space-y-4">
    <!-- Tools section (libre mode only) -->
    {#if session?.mode === "libre"}
      <Card class="p-3.5 space-y-3">
        <p class="text-micro font-medium uppercase tracking-wider text-muted-foreground/50">
          {isOperator ? $t("tools.select.title_operator") : $t("tools.select.title_builder")}
        </p>

        {#each TOOL_GROUPS as group (group.id)}
          {@const groupTools = TOOL_CATALOG.filter(t => t.group === group.id)}
          {@const state = getGroupState(group, enabledTools)}
          {@const GroupIcon = group.icon}
          {@const isCollapsed = collapsedGroups.has(group.id)}
          {@const enabledCount = groupTools.filter(t => enabledTools.has(t.id)).length}
          <div class="space-y-0.5">
            <div class="flex items-center justify-between px-1 py-1 rounded hover:bg-muted/20 transition-colors">
              <Button variant="ghost" size="sm"
                class="flex items-center gap-1.5 flex-1 text-left"
                onclick={() => {
                  const next = new Set(collapsedGroups);
                  if (next.has(group.id)) next.delete(group.id); else next.add(group.id);
                  collapsedGroups = next;
                }}
                data-testid="config-group-header-{group.id}"
              >
                <GroupIcon class="h-3 w-3 text-muted-foreground/60" />
                <span class="text-micro font-semibold uppercase tracking-wider text-muted-foreground/60">
                  {isOperator ? $t(group.labelOperatorKey) : $t(group.labelBuilderKey)}
                </span>
                {#if isCollapsed}
                  <span class="text-micro text-muted-foreground/40 font-normal normal-case tracking-normal">
                    {state === "all" ? "· all" : state === "some" ? `· ${enabledCount}/${groupTools.length}` : "· none"}
                  </span>
                {/if}
                <ChevronDown
                  class="h-3 w-3 text-muted-foreground/30 transition-transform duration-fast ml-auto {isCollapsed ? '' : 'rotate-180'}"
                />
              </Button>
              {#if !isCollapsed && !isReadOnly}
                <button
                  class="text-micro-xs text-muted-foreground/30 hover:text-primary transition-colors px-1.5 ml-1"
                  onclick={() => { enabledTools = toggleGroup(group, enabledTools); }}
                  data-testid="config-group-toggle-{group.id}"
                >
                  {state === "all" ? "− none" : "+ all"}
                </button>
              {/if}
            </div>

            {#if !isCollapsed}
              {#each groupTools as tool (tool.id)}
                {@const checked = enabledTools.has(tool.id)}
                {@const ToolIcon = tool.icon}
                {@const sensitive = isSensitiveFamily(tool.family)}
                <div class="flex items-center justify-between rounded-lg px-3 py-2 glass-surface {isReadOnly ? 'opacity-50' : ''}">
                  <div class="flex items-center gap-2.5 min-w-0 flex-1">
                    <ToolIcon
                      class="h-3.5 w-3.5 shrink-0 {sensitive ? 'text-warning/70' : 'text-muted-foreground'}"
                      data-testid="config-tool-icon-{tool.id}"
                      data-sensitive={sensitive}
                    />
                    <div class="min-w-0">
                      <div class="flex items-center gap-1.5">
                        <span class="text-xs font-medium truncate">
                          {isOperator ? $t(tool.labelOperatorKey) : tool.id}
                        </span>
                        {#if sensitive && !isOperator}
                          <TriangleAlert
                            class="h-2.5 w-2.5 text-warning/60 shrink-0"
                            data-testid="config-tool-sensitive-{tool.id}"
                          />
                        {/if}
                      </div>
                      <p class="text-micro text-muted-foreground/50 leading-tight mt-0.5">
                        {isOperator ? $t(tool.descOperatorKey) : $t(tool.descBuilderKey)}
                      </p>
                    </div>
                  </div>
                  <Toggle
                    size="sm"
                    {checked}
                    onchange={(v) => {
                      const next = new Set(enabledTools);
                      if (v) next.add(tool.id); else next.delete(tool.id);
                      enabledTools = next;
                    }}
                    disabled={isReadOnly}
                    data-testid="config-tool-{tool.id}"
                  />
                </div>
              {/each}
            {/if}
          </div>
        {/each}
      </Card>
    {/if}

    <!-- User memory toggle -->
    <Card class="p-3.5 space-y-2.5">
      <p class="text-micro font-medium uppercase tracking-wider text-muted-foreground/50">
        {$t("chat.memory_section")}
      </p>
      <div class="space-y-1.5">
        <label for="config-profile-select" class="text-xs font-medium">
          {$t("chat.config.profile_label")}
        </label>
        <Select
          id="config-profile-select"
          size="sm"
          value={profileSelection(selectedProfile, profiles)}
          onchange={(e: Event) => {
            selectedProfile = (e.currentTarget as HTMLSelectElement).value;
          }}
          disabled={isReadOnly}
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
        <p class="text-micro-xs text-muted-foreground/60">{$t("chat.config.profile_hint")}</p>
      </div>
      <div class="flex items-center justify-between rounded-lg px-3 py-2 glass-surface">
        <div class="flex items-center gap-2.5">
          <Brain class="h-3.5 w-3.5 text-secondary" />
          <span class="text-xs font-medium">{memoryToggleLabel}</span>
        </div>
        <Toggle
          size="sm"
          checked={$useUserMemory}
          disabled={isClosed}
          onchange={handleProfileSwitch}
          data-testid="toggle-user-memory"
        />
      </div>
    </Card>

    {#if saveError}
      <div class="rounded-lg border border-destructive/30 bg-destructive/5 px-3.5 py-2.5 animate-fade-in">
        <p class="text-xs text-destructive" data-testid="config-error">{saveError}</p>
      </div>
    {/if}
  </div>

  {#if !isReadOnly}
    <div class="px-5 py-4">
      <Separator />
      <Button class="w-full bg-primary" onclick={handleSave} disabled={saving} data-testid="config-save">
        <Save class="mr-2 h-4 w-4" />
        {saving ? $t("common.submitting") : $t("chat.config_save")}
      </Button>
    </div>
  {/if}
</div>
