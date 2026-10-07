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
  "ai.installed.group", // {label} · {count}
  "ai.panel.inference.structured.placeholder", // { "type": "object", "properties": {} }
  "authorizations.apollia_name", // Apollia
  "chat.generation.seconds", // {value} s
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
  "connections.native_google_name", // Google Workspace
  "connections.native_microsoft_description", // Outlook Mail, Outlook Calendar, OneDrive
  "connections.native_microsoft_name", // Microsoft 365
  "dev.connections.custom_transport_http", // Streamable HTTP (MCP 2025-11-25)
  "dev.connections.custom_transport_sse", // SSE (legacy 2024-11-05)
  "dev.workflows.depth", // depth
  "dev.workflows.parent_run", // parent_run_id
  "dev.workflows.run_id", // run_id
  "dev.workflows.workflow_id", // workflow_id
  "notifications.field_headers_placeholder", // {"Authorization": "Bearer ..."}
  "notifications.field_url_placeholder", // https://hooks.example.com/notify
  "observability.timeline_window_30min", // 30 min
  "observability.type_llm", // LLM
  "settings.integrations.drive.placeholder", // Apollia
  "settings.integrations.field.client_id", // Client ID
  "settings.integrations.placeholder.api_key", // AIzaSy…
  "settings.integrations.placeholder.client_id_google", // 1234567890-abcdefgh.apps.googleusercontent.com
  "settings.integrations.placeholder.client_secret", // GOCSPX-…
  "settings.language_en", // English
  "settings.language_fr", // Français
  "settings.model_defaults.images.unit", // pixels
  "settings.model_defaults.unload.unit", // minutes
  "settings.hardware.gpu", // GPU
  "settings.stt_cuda", // CUDA
  "settings.stt_metal", // Metal
  "settings.tools_page.bash_executor_title", // Bash
  "settings.tools_page.python_executor_title", // Python
  "tools.body.exit_code", // exit {code}
  "tools.body.progress_fraction", // {done} / {total}
  "tools.body.url_label", // URL
  "tools.catalog.file_grep.desc_builder", // file_grep - regex, context_lines, max_results - SandboxProfile::ReadOnly
  "tools.labels.connector", // {provider}
  "transcriptions.source_api", // API
];

/**
 * Translatable prose whose French translation is the same string: either the
 * word is spelled the same in both languages ("Configuration", "Description",
 * "Navigation"), or it is the English term the French interface deliberately
 * keeps ("Backend", "Endpoint", "Builder", "Chat").
 */
export const IDENTICAL_BY_DECISION: string[] = [
  "ai.capability.vision.label", // Vision
  "ai.category.embeddings", // Embeddings
  "ai.installed.col_actions", // Actions
  "ai.kind.embeddings", // Embeddings
  "ai.panel.auto", // Auto
  "ai.panel.info.architecture", // Architecture
  "ai.panel.info.format", // Format
  "ai.panel.preset.label", // Preset
  "a11y.actions_menu", // Actions
  "agent_adjust.grant_group_agents", // Agents
  "agent_adjust.grant_group_workflows", // Workflows
  "agent_adjust.mission_label", // Mission
  "agent_adjust.preset_label", // Preset
  "agent_detail.messages_title", // Messages
  "agents.column_agents", // Agents
  "agents.column_workflows", // Workflows
  "agents.install.done_agents_many", // {n} agents
  "agents.install.done_agents_one", // {n} agent
  "agents.install.done_triggers_many", // {n} triggers
  "agents.install.done_triggers_one", // {n} trigger
  "agents.install.endpoint", // Endpoint
  "agents.install.section_agents", // Agents ({n})
  "agents.kicker_agent", // Agent
  "agents.logs", // Logs
  "agents.title", // Agents
  "automations.status.active", // Active
  "automations.target_prefix", // Assistant
  "automations.wizard.step_agent", // Assistant
  "batch_launch.agent.title", // Agent
  "batch_launch.source.title", // Source
  "chat.activity.sources", // Sources
  "chat.assistant", // Assistant
  "chat.chips.vision", // Vision
  "chat.citations_aria", // Citations
  "chat.command_group.agents", // Agents
  "chat.config_title", // Configuration
  "chat.conversation_configure", // Configuration
  "chat.conversations_aria", // Conversations
  "chat.export.meta_mode", // Mode
  "chat.export.meta_session", // Session
  "chat.export.role_assistant", // Assistant
  "chat.journal.agent", // Agent
  "chat.mode_auto", // Auto
  "chat.planMode.phaseLabel", // Phase
  "chat.quickpicker.agents_section", // Agents
  "chat.quickpicker.docs_link", // Documentation
  "chat.shortcut_group.chat", // Chat
  "chat.shortcut_group.messages", // Messages
  "chat.shortcut_group.navigation", // Navigation
  "chat.status_active", // Active
  "chat.system_prompt", // Instructions
  "chat.target_selector.group_agents", // Agents
  "chat.title", // Chat
  "commandPalette.groups.actions", // Actions
  "commandPalette.groups.pages", // Pages
  "commandPalette.groups.sessions", // Sessions
  "connections.badge_status_attention", // attention
  "connections.custom_test_ok", // Test OK - {result}
  "connections.mcp_settings.transport", // Transport
  "connections.mailbox.port_label", // Port
  "detailSidebar.tab_guide", // Guide
  "dev.ai.panel.type.embeddings.label", // Embeddings
  "dev.connections.custom_transport_label", // Transport
  "connections.settings_provider_title", // Provider
  "connections.status_chip_attention", // Attention
  "dev.developer.docs.admonition.danger", // Danger
  "dev.developer.docs.admonition.info", // Info
  "dev.developer.docs.admonition.note", // Note
  "dev.developer.views.documentation", // Documentation
  "dev.journal.arguments", // Arguments
  "guide.doc.breadcrumb", // Documentation
  "integrations.wizard.local_loopback_badge", // Local
  "journal.detail.action", // Action
  "journal.detail.agent", // Agent
  "journal.filter.agent", // Agent
  "journal.filter.nature", // Nature
  "journal.nature.action", // Actions
  "journal.title", // Journal
  "library.detail.agents", // Agents
  "library.detail.workflows", // Workflows
  "library.kind.agent", // agent
  "library.kind.workflow", // workflow
  "library.uninstall.leaving.agents", // {count, plural, one {# agent} other {# agents}}
  "library.uninstall.leaving.workflows", // {count, plural, one {# workflow} other {# workflows}}
  "memory.meta_expiration", // Expiration
  "memory.meta_namespace", // Namespace
  "memory.meta_type", // Type
  "memory.namespaces.cat_agent", // Agents
  "memory.namespaces.cat_agent_header", // Agents
  "memory.namespaces.title", // Namespaces
  "nav.agents", // Agents
  "nav.chat", // Chat
  "nav.sidebar.agents", // Agents
  "nav.sidebar.chat", // Chat
  "nav.sidebar.transcriptions", // Transcriptions
  "nav.transcriptions", // Transcriptions
  "notifications.field_type", // Type
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
  "packages.preview.content.kind_agent", // Agent
  "packages.preview.content.kind_workflow", // Workflow
  "packages.preview.version", // version {version}
  "plan_mode.history_pause", // Pause
  "plan_session.origin_initial", // Initial
  "plan_session.tab_journal", // Journal
  "plan_session.tab_plan", // Plan
  "policy.badge.active", // Active
  "policy.section.journal.title", // Journal
  "policy.signature.title", // Signature
  "project_migration.row_documents_title", // Documents
  "project_migration.row_instructions_title", // Instructions
  "project_migration.where_chat_title", // Chat
  "project_migration.where_sources_title", // Agents › Apollia › Sources
  "review.filter.questions", // Questions
  "review.question.badge", // Question
  "settings.about.contact_title", // Contact
  "settings.about.docs_title", // Documentation
  "settings.chat.conversations_title", // Conversations
  "settings.credential.absent", // Absent
  "settings.developer.mcp.kind_agent", // Agent
  "settings.developer.mcp.kind_workflow", // Workflow
  "settings.engines.channel_stable", // Stable
  "settings.developer.on_demand.idle_unit", // minutes
  "settings.general.application_title", // Application
  "settings.general.channel_stable", // Stable
  "settings.general.interface_title", // Interface
  "settings.hardware.architecture", // Architecture
  "settings.hardware.compatible", // Compatible
  "settings.nav.chat", // Chat
  "settings.nav.configuration", // Configuration
  "settings.nav.mobile_open", // Sections
  "settings.nav.notifications", // Notifications
  "settings.profile.source.agent", // agent
  "settings.profile.source.onboarding", // onboarding
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
  "triggers.workflow_label", // Workflow
  "workflows.kicker", // Workflow
];

/** The two lists together, as the guard reads them. */
export const IDENTICAL_ACROSS_LOCALES: ReadonlySet<string> = new Set([
  ...IDENTICAL_BY_NATURE,
  ...IDENTICAL_BY_DECISION,
]);
