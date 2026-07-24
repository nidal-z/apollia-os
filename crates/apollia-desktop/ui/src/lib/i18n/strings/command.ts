/** Command palette keys (+). */
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

/** Command palette v2 keys. */
export const COMMAND_PALETTE_KEYS = {
  placeholder: {
    all: "commandPalette.placeholder.all",
  },
  groups: {
    recent: "commandPalette.groups.recent",
    pages: "commandPalette.groups.pages",
    actions: "commandPalette.groups.actions",
    sessions: "commandPalette.groups.sessions",
    settings: "commandPalette.groups.settings",
    help: "commandPalette.groups.help",
  },
  actions: {
    createAgent: "commandPalette.actions.createAgent",
    createTrigger: "commandPalette.actions.createTrigger",
    runPipeline: "commandPalette.actions.runPipeline",
    toggleMode: "commandPalette.actions.toggleMode",
    toggleCompanion: "commandPalette.actions.toggleCompanion",
  },
  help: {
    shortcuts: "commandPalette.help.shortcuts",
    help: "commandPalette.help.help",
    about: "commandPalette.help.about",
    docs: "commandPalette.help.docs",
    resetOnboarding: "commandPalette.help.resetOnboarding",
    quit: "commandPalette.help.quit",
    toggleDevConsole: "commandPalette.help.toggleDevConsole",
  },
  tip: "commandPalette.tip",
} as const;
