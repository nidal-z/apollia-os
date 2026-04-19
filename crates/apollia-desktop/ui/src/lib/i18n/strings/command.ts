/** Command palette keys (US-SP42-017). */
export const COMMAND_KEYS = {
  placeholder: "command.placeholder",
  empty: "command.empty",
  group: {
    navigation: "command.group.navigation",
    agents: "command.group.agents",
    preferences: "command.group.preferences",
    help: "command.group.help",
  },
  seed: {
    navDashboard: "command.seed.nav_dashboard",
    navAgents: "command.seed.nav_agents",
    navTasks: "command.seed.nav_tasks",
    navChat: "command.seed.nav_chat",
    navIntegrations: "command.seed.nav_integrations",
    navSettings: "command.seed.nav_settings",
    agentsStartAll: "command.seed.agents_start_all",
    agentsStopAll: "command.seed.agents_stop_all",
    toggleDark: "command.seed.toggle_dark",
    paletteDocs: "command.seed.palette_docs",
  },
} as const;
