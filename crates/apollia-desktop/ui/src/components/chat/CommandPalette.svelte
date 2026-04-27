<script lang="ts">
  /**
   * Chat-route command palette wrapper.
   *
   * The global palette UI lives in `App.svelte` (mounted via the
   * `Command` primitive), which makes it available everywhere. This
   * wrapper enriches the palette while the chat route is mounted by
   * registering chat-specific entries:
   *   - recent open sessions (jump to)
   *   - chat templates (open new picker pre-selected)
   *   - available agents (start a new conversation with)
   *   - chat slash commands (/clear, /export, …) for discoverability
   *
   * Registrations are scoped to the component lifecycle: `onDestroy`
   * unregisters every entry so navigating away from the chat keeps
   * the palette focused on global commands.
   */
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { MessageSquare, Bot, BookOpen, Slash } from "lucide-svelte";
  import {
    registerCommand,
    unregisterCommand,
    type CommandItem,
  } from "$lib/stores/commandPalette";
  import { chatSessions, agents } from "$lib/stores/sse";
  import { CHAT_TEMPLATES } from "$lib/templates/chatTemplates";
  import { SLASH_COMMANDS } from "$lib/chat/slashCommands";

  interface Props {
    /** Open an existing session (called by palette session entries). */
    onselectSession: (sessionId: string) => void;
    /** Open the new-chat picker, optionally pre-selecting a template/agent. */
    onnewChat: (preset?: { templateId?: string; agentName?: string }) => void;
    /** Append a slash-command marker into the current chat input. */
    onslashCommand?: (cmdId: string) => void;
  }

  let { onselectSession, onnewChat, onslashCommand }: Props = $props();

  const RECENT_SESSION_LIMIT = 8;
  let registeredIds: string[] = [];

  function refresh(): void {
    if (registeredIds.length > 0) {
      unregisterCommand(...registeredIds);
      registeredIds = [];
    }

    const entries: CommandItem[] = [];
    const tt = get(t);

    const sessionsLabel = tt("chat.command_group.sessions");
    const recent = get(chatSessions)
      .filter((s) => s.status !== "closed")
      .slice(0, RECENT_SESSION_LIMIT);
    for (const session of recent) {
      const title = session.title ?? tt("chat.untitled_session");
      entries.push({
        id: `chat.session.${session.id}`,
        label: title,
        description: session.id.slice(0, 8),
        icon: MessageSquare,
        group: sessionsLabel,
        keywords: ["session", "chat", title],
        action: () => onselectSession(session.id),
      });
    }

    const templatesLabel = tt("chat.command_group.templates");
    for (const tpl of CHAT_TEMPLATES) {
      const title = tt(tpl.titleKey);
      entries.push({
        id: `chat.template.${tpl.id}`,
        label: title,
        description: tt(tpl.descriptionKey),
        icon: BookOpen,
        group: templatesLabel,
        keywords: ["template", tpl.id, ...(tpl.tools ?? [])],
        action: () => onnewChat({ templateId: tpl.id }),
      });
    }

    const agentsLabel = tt("chat.command_group.agents");
    for (const agent of get(agents)) {
      entries.push({
        id: `chat.agent.${agent.name}`,
        label: agent.name,
        description: tt("chat.command.start_with_agent"),
        icon: Bot,
        group: agentsLabel,
        keywords: ["agent", agent.name],
        action: () => onnewChat({ agentName: agent.name }),
      });
    }

    const slashLabel = tt("chat.command_group.slash");
    for (const slash of SLASH_COMMANDS) {
      entries.push({
        id: `chat.slash.${slash.id}`,
        label: `/${slash.name}`,
        description: tt(slash.descriptionKey),
        icon: Slash,
        group: slashLabel,
        keywords: ["slash", slash.id, ...(slash.aliases ?? [])],
        action: () => onslashCommand?.(slash.id),
      });
    }

    if (entries.length > 0) {
      registerCommand(...entries);
      registeredIds = entries.map((e) => e.id);
    }
  }

  onMount(() => {
    refresh();
    const unsubSessions = chatSessions.subscribe(() => refresh());
    const unsubAgents = agents.subscribe(() => refresh());
    return () => {
      unsubSessions();
      unsubAgents();
    };
  });

  onDestroy(() => {
    if (registeredIds.length > 0) unregisterCommand(...registeredIds);
  });
</script>
