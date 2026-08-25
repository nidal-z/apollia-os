/**
 * The catalogue entries whose French side is the English side, on purpose.
 *
 * Nothing compared the two locales on value until now: `i18n-catalogue-parity.test.ts`
 * asks that a key exist on both sides, never that the two sides say different
 * things, and `call-site-keys.test.ts` compared exactly sixteen keys. Four
 * labels were English in the French interface for that reason
 * (`tasks.tech_agent_name`, `tasks.tech_task_id`, `agents.install.section_triggers`,
 * `projects.kicker`), and none of them could be seen among the several hundred
 * entries that are legitimately the same string.
 *
 * So the rule is stated the other way round: a key carrying a word must differ
 * between the two locales unless it is named here, in one of two lists whose
 * membership test a reader can apply without knowing the screen.
 */

/**
 * Not translatable prose: a brand or product name, a protocol, a unit, an
 * identifier, a code fragment, or a placeholder the runtime fills.
 */
export const IDENTICAL_BY_NATURE: string[] = [
  "connections.capabilities.group_gdocs", // Google Docs
  "connections.capabilities.group_gdrive", // Google Drive
  "connections.capabilities.group_gforms", // Google Forms
  "connections.capabilities.group_gmail", // Gmail
  "connections.capabilities.group_gsheets", // Google Sheets
  "connections.capabilities.group_gslides", // Google Slides
  "connections.capabilities.group_gtasks", // Google Tasks
  "connections.capabilities.group_onedrive", // OneDrive
  "connections.capabilities.group_outlook_mail", // Outlook Mail
  "connections.capabilities.group_youtube", // YouTube
  "connections.custom_transport_http", // Streamable HTTP (MCP 2025-11-25)
  "connections.custom_transport_sse", // SSE (legacy 2024-11-05)
  "connections.native_microsoft_description", // Outlook Mail, Outlook Calendar, OneDrive
  "dashboard.headline_greeting", // {greeting}.
  "hitl.fs.preview_mode_label", // {before} → {after}
  "inbox.activity.filter.llm", // LLM
  "notifications.field_headers_placeholder", // {"Authorization": "Bearer ..."}
  "notifications.field_url_placeholder", // https://hooks.example.com/notify
  "observability.timeline_window_30min", // 30 min
  "observability.type_llm", // LLM
  "onboarding_permissions.scope_label", // {scope}
  "projects.provider_type_git", // Git
  "settings.integrations.drive.placeholder", // Apollia
  "settings.integrations.field.client_id", // Client ID
  "settings.integrations.placeholder.api_key", // AIzaSy…
  "settings.integrations.placeholder.client_id_google", // 1234567890-abcdefgh.apps.googleusercontent.com
  "settings.integrations.placeholder.client_secret", // GOCSPX-…
  "settings.integrations.provider.google", // Google Workspace
  "settings.integrations.provider.microsoft", // Microsoft 365
  "settings.language_en", // English
  "settings.language_fr", // Français
  "settings.llm_dialog.field_top_k", // Top-K
  "settings.llm_dialog.field_top_p", // Top-P
  "settings.model_hub.detail.param_top_k", // top_k
  "settings.model_hub.detail.param_top_p", // top_p
  "settings.model_hub.filters.lang_de", // Deutsch
  "settings.model_hub.filters.lang_en", // English
  "settings.model_hub.filters.lang_es", // Español
  "settings.model_hub.filters.lang_fr", // Français
  "settings.model_hub.filters.lang_pt", // Português
  "settings.model_hub.hardware.ram", // RAM
  "settings.profile.language.en", // English
  "settings.profile.language.fr", // Français
  "settings.profile.llm.anthropic", // Anthropic
  "settings.profile.llm.bedrock", // AWS Bedrock
  "settings.profile.llm.local", // Local (llama.cpp)
  "settings.profile.llm.ollama", // Ollama
  "settings.profile.llm.openai", // OpenAI
  "settings.profile.llm.vertex", // Vertex AI
  "settings.profile.placeholder.tech_stack", // Python, TypeScript, React, Postgres…
  "settings.profile.placeholder.tools_daily", // Excel, Notion, Salesforce, Gmail, VS Code…
  "settings.profile.sector.ecommerce", // E-commerce
  "settings.profile.sector.fintech", // Fintech
  "settings.stt_cuda", // CUDA
  "settings.stt_metal", // Metal
  "settings.tools_page.bash_executor_title", // Bash
  "settings.tools_page.python_executor_title", // Python
  "tasks.tech_agent_id", // Agent ID
  "tools.body.exit_code", // exit {code}
  "tools.body.progress_fraction", // {done} / {total}
  "tools.body.url_label", // URL
  "tools.catalog.file_grep.desc_builder", // file_grep - regex, context_lines, max_results - SandboxProfile::ReadOnly
  "tools.labels.connector", // {provider}
  "tour.band.progress", // {done} / {total}
  "transcriptions.source_api", // API
];

/**
 * Translatable prose whose French translation is the same string: either the
 * word is spelled the same in both languages ("Configuration", "Description",
 * "Navigation"), or it is the English term the French interface deliberately
 * keeps ("Backend", "Endpoint", "Builder", "Chat").
 */
export const IDENTICAL_BY_DECISION: string[] = [
  "a11y.actions_menu", // Actions
  "agent_detail.execution_mode_direct", // Direct
  "agent_detail.llm_backend", // Backend
  "agent_detail.messages_title", // Messages
  "agent_detail.tags_title", // Tags
  "agents.agents_word", // agents
  "agents.install.done_agents_many", // {n} agents
  "agents.install.done_agents_one", // {n} agent
  "agents.install.done_triggers_many", // {n} triggers
  "agents.install.done_triggers_one", // {n} trigger
  "agents.install.endpoint", // Endpoint
  "agents.install.section_agents", // Agents ({n})
  "agents.logs", // Logs
  "agents.page_title", // Assistants
  "agents.tab_agents", // Agents
  "agents.tab_triggers", // Triggers
  "agents.triggers_word", // triggers
  "automations.col_assistant", // Assistant
  "automations.status.active", // Active
  "automations.target_prefix", // Assistant
  "automations.wizard.step_agent", // Assistant
  "chat.activity.sources", // Sources
  "chat.assistant", // Assistant
  "chat.citations_aria", // Citations
  "chat.command_group.agents", // Agents
  "chat.config_title", // Configuration
  "chat.conversation_configure", // Configuration
  "chat.conversations_aria", // Conversations
  "chat.export.meta_mode", // Mode
  "chat.export.meta_session", // Session
  "chat.export.role_assistant", // Assistant
  "chat.journal.agent", // Agent
  "chat.journal_mode_builder", // Builder
  "chat.legend_messages", // Messages
  "chat.mode_auto", // Auto
  "chat.planMode.phaseLabel", // Phase
  "chat.quickpicker.agents_section", // Agents
  "chat.quickpicker.docs_link", // Documentation
  "chat.shortcut_group.chat", // Chat
  "chat.shortcut_group.messages", // Messages
  "chat.shortcut_group.navigation", // Navigation
  "chat.status_active", // Active
  "chat.system_prompt", // Instructions
  "chat.title", // Chat
  "commandPalette.groups.actions", // Actions
  "commandPalette.groups.pages", // Pages
  "commandPalette.groups.sessions", // Sessions
  "companion.title", // Companion
  "connections.badge_status_attention", // attention
  "connections.custom_test_ok", // Test OK - {result}
  "connections.custom_transport_label", // Transport
  "connections.mcp_settings.transport", // Transport
  "connections.settings_provider_title", // Provider
  "connections.status_chip_attention", // Attention
  "inbox.chips.questions", // Questions
  "inbox.filter_by_agent", // Agent
  "inbox.history.origin_chat", // Chat
  "inbox.history.session", // Session
  "inbox.row.type_question", // question
  "inbox.row.type_trigger", // trigger
  "integrations.wizard.local_loopback_badge", // Local
  "integrations.wizard.step_test", // Test
  "llm.ping_ok", // Ping OK ({latency}ms)
  "llm.table.backend", // Backend
  "llm.table.tokens", // Tokens
  "llm.table.total", // Total
  "memory.meta_expiration", // Expiration
  "memory.meta_namespace", // Namespace
  "memory.meta_type", // Type
  "memory.namespaces.cat_agent", // Agents
  "memory.namespaces.cat_agent_header", // Agents
  "memory.namespaces.title", // Namespaces
  "nav.builder_cluster", // Inspection
  "nav.chat", // Chat
  "nav.notifications", // Notifications
  "nav.sidebar.chat", // Chat
  "nav.sidebar.notifications", // Notifications
  "nav.sidebar.transcriptions", // Transcriptions
  "nav.transcriptions", // Transcriptions
  "notifications.field_type", // Type
  "notifications.field_type_desktop", // Desktop
  "notifications.field_type_webhook", // Webhook
  "notifications.title", // Notifications
  "observability.agent", // Agent
  "observability.filter", // Type
  "observability.hooks_col_timeout", // Timeout
  "observability.plan_cache.cache_hits", // Hits
  "observability.plan_cache.cache_misses", // Misses
  "observability.plan_mutation.field.description", // Description
  "observability.tab_hooks", // Hooks
  "observability.table.agent", // Agent
  "observability.table.arguments", // Arguments
  "observability.task_timeline_fact_backend", // Backend
  "onboarding.ai_setup.backends_count", // {count, plural, one {# backend} other {# backends}}
  "onboarding.ai_setup.gated", // gated
  "onboarding.profile_selector.builder_title", // Builder
  "onboarding_stt.mic_label", // Microphone
  "onboarding_welcome.feature_local_title", // Local-first
  "plan_mode.history_pause", // Pause
  "plan_session.origin_initial", // Initial
  "plan_session.tab_journal", // Journal
  "plan_session.tab_plan", // Plan
  "projects.card_agents", // {n, plural, one {# agent} other {# agents}}
  "projects.field_description", // Description
  "projects.provider_field_timeout_ms", // Timeout (ms)
  "projects.provider_field_type", // Type
  "projects.provider_type_script", // Script
  "projects.provider_type_style", // Style
  "projects.settings_field_description", // Description
  "projects.tab_agents", // Agents
  "projects.tab_conversations", // Conversations
  "settings.about.contact_title", // Contact
  "settings.about.diag_transcription", // Transcription
  "settings.about.docs_title", // Documentation
  "settings.about.kv_stt", // Transcription
  "settings.about.kv_version", // Version
  "settings.appearance.options_title", // Options
  "settings.credential.absent", // Absent
  "settings.llm_backend.provider_local", // local
  "settings.llm_dialog.field_endpoint", // Endpoint
  "settings.mode_builder", // Builder
  "settings.model_hub.detail.param_max_tokens", // max tokens
  "settings.model_hub.detail.param_temperature", // temperature
  "settings.model_hub.filters.type_base", // Base
  "settings.model_hub.filters.type_instruct", // Instruct / Chat
  "settings.nav.configuration", // Configuration
  "settings.nav.mobile_open", // Sections
  "settings.permissions.add.action_label", // Action
  "settings.permissions.scope_agent", // Chat / agent
  "settings.permissions.session_badge", // Session
  "settings.profile.proficiency.expert", // Expert
  "settings.profile.source.agent", // agent
  "settings.profile.source.onboarding", // onboarding
  "settings.profile.team_size.solo", // Solo
  "settings.shortcuts.category.chat", // Chat
  "settings.shortcuts.category.companion", // Companion
  "settings.shortcuts.category.global", // Global
  "settings.shortcuts.category.navigation", // Navigation
  "settings.shortcuts.scope.global_label", // Global
  "settings.stt.section.performance_title", // PERFORMANCE
  "settings.stt_backend", // Backend
  "settings.stt_input_device", // Microphone
  "settings.stt_microphone", // Microphone
  "settings.system_version", // Version
  "settings.tool_config.field.timeout_secs", // Timeout (s)
  "sidebar.mode.builder.label", // Builder
  "tasks.col_agent", // Agent
  "tasks.tab_trace", // Trace
  "tools.body.content_type_label", // Type
  "tools.body.http_status_info", // Information
  "tools.body.http_status_redirect", // Redirection
  "tools.groups.network.label_operator", // Web
  "tools.labels.ask_user", // Question
  "topbar.route.design", // Design
  "topbar.route.design_motion", // Motion
  "trace.args_label", // Arguments
  "transcriptions.title", // Transcriptions
  "triggers.agent_label", // Agent
  "triggers.field_secret", // Secret
  "triggers.field_target_agent", // Agent
  "triggers.interval_unit_minutes", // minutes
];

/** The two lists together, as the guard reads them. */
export const IDENTICAL_ACROSS_LOCALES: ReadonlySet<string> = new Set([
  ...IDENTICAL_BY_NATURE,
  ...IDENTICAL_BY_DECISION,
]);
