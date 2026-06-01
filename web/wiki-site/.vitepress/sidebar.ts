// Sidebar dérivée manuellement de docs/wiki/_Sidebar.md (format GitHub Wiki).
// Édité ici plutôt que parsé dynamiquement pour permettre le réordonnancement.
//
// Convention : les chemins n'ont pas d'extension `.md` (cleanUrls actif).

import type { DefaultTheme } from 'vitepress'

export const sidebar: DefaultTheme.Sidebar = [
  {
    text: 'Démarrage rapide',
    items: [
      { text: 'Installation (5 min)', link: '/INSTALL-Quickstart' },
      { text: 'Premier agent (5 min)', link: '/Agents-Quickstart' },
    ],
  },
  {
    text: 'Guides Agents',
    collapsed: false,
    items: [
      { text: 'Matrice de décision - Capabilities', link: '/Decision-Matrix-Capabilities' },
      { text: 'Worker Agent Pattern', link: '/Worker-Agent-Pattern' },
      { text: 'Community Agent Registry', link: '/Community-Agent-Registry' },
      { text: 'Tutoriel Hello Agent', link: '/Agents-Tutoriel-Hello-Agent' },
      { text: 'RuntimeContext (ctx.*)', link: '/Agents-RuntimeContext-Guide' },
      { text: 'Mode Orchestré (ORIA)', link: '/Agents-Mode-Orchestre' },
      { text: 'Adapter LangGraph / CrewAI', link: '/Agents-Adapter-Existants' },
      { text: 'Python SDK', link: '/Agents-SDK-Guide' },
      { text: 'ContextBootstrap', link: '/Agents-ContextBootstrap-Guide' },
      { text: 'Onboarding', link: '/Agents-Onboarding-Guide' },
      { text: 'Bonnes pratiques', link: '/Agents-Bonnes-Pratiques' },
      { text: 'Troubleshooting', link: '/Agents-Troubleshooting' },
    ],
  },
  {
    text: 'Référence AIP',
    items: [
      { text: 'AIP Specification', link: '/Briques-AIP-Specification' },
    ],
  },
  {
    text: 'Architecture',
    collapsed: true,
    items: [
      { text: 'Principes', link: '/Architecture-Principes' },
      { text: 'Vue d\'ensemble & AIP', link: '/Architecture-Vue-Ensemble' },
      { text: 'Modèle Acteur Tokio', link: '/Architecture-Modele-Acteur' },
      { text: 'Machines d\'état', link: '/Architecture-Machines-Etat' },
      { text: 'MCP · A2A · ACP', link: '/Architecture-Protocoles-Standards' },
    ],
  },
  {
    text: 'Briques fondamentales',
    collapsed: false,
    items: [
      { text: 'Tool Registry', link: '/Briques-Tool-Registry' },
      { text: 'Référence des outils natifs', link: '/Outils-Reference' },
      { text: 'Client MCP', link: '/Briques-MCP' },
      { text: 'Memory Engine', link: '/Briques-Memory-Engine' },
      { text: 'User Memory', link: '/Briques-User-Memory' },
      { text: 'ORIA Engine', link: '/Briques-ORIA-Engine' },
      { text: 'LLM Backend', link: '/Briques-LLM-Backend' },
      { text: 'Auth OAuth2 PKCE', link: '/Briques-Auth' },
      { text: 'Triggers Engine', link: '/Briques-Triggers' },
      { text: 'Notifications Engine', link: '/Briques-Notifications' },
      { text: 'Pipelines Engine', link: '/Briques-Pipelines' },
      { text: 'Permissions', link: '/Briques-Permissions' },
      { text: 'Workspace', link: '/Briques-Workspace' },
      { text: 'Runtime Core', link: '/Briques-Runtime-Core' },
      { text: 'Application Desktop', link: '/Briques-Desktop' },
      { text: 'Onboarding - Spec système', link: '/Onboarding-System' },
      { text: 'Onboarding - Étapes du tour', link: '/Onboarding-Tour-Steps' },
      { text: 'Speech-to-Text', link: '/Briques-STT' },
      { text: 'Chat', link: '/Briques-Chat' },
      { text: 'Apollia CLI', link: '/Briques-CLI' },
    ],
  },
  {
    text: 'Installation & Config',
    items: [
      { text: 'Installation complète', link: '/INSTALL' },
      { text: 'Production Linux', link: '/INSTALL-Production' },
      { text: 'apollia.toml', link: '/Config-apollia-toml' },
    ],
  },
  {
    text: 'API & Intégration',
    collapsed: false,
    items: [
      { text: 'API HTTP - Index', link: '/API-HTTP-Reference' },
      { text: 'Agents, Chat, LLM, A2A', link: '/API-HTTP-Agents' },
      { text: 'Triggers, Pipelines, Notifications', link: '/API-HTTP-Workspace' },
      { text: 'Observability, STT, MCP', link: '/API-HTTP-Observability' },
      { text: 'MCP - Guide utilisateur', link: '/MCP-Guide-Utilisateur' },
      { text: 'Intégrations - Guide desktop', link: '/Integrations-Guide' },
      { text: 'MCP - Intégration technique', link: '/MCP-Integration' },
      { text: 'A2A / ACP', link: '/A2A-ACP-Alignement' },
    ],
  },
  {
    text: 'Sécurité',
    items: [
      { text: 'Local-First', link: '/Securite-Local-First' },
      { text: 'Sandbox Isolation', link: '/Securite-Sandbox-Isolation' },
      { text: 'Guardrails', link: '/Securite-Guardrails' },
      { text: 'Moteur de Permissions', link: '/Briques-Permissions' },
    ],
  },
  {
    text: 'Ops',
    items: [
      { text: 'Exploitation & Debug', link: '/Ops-Exploitation-et-Debug' },
      { text: 'Dashboard Observabilité', link: '/Dashboard-Observabilite' },
    ],
  },
  {
    text: 'Référence',
    items: [
      { text: 'Glossaire', link: '/Glossaire' },
      { text: 'Design System', link: '/DESIGN-SYSTEM' },
    ],
  },
  {
    text: 'Vision',
    collapsed: true,
    items: [
      { text: 'Pourquoi Apollia', link: '/Pourquoi-Apollia' },
      { text: 'Pivot & Renouveau', link: '/Vision-Pivot-et-Renouveau' },
      { text: 'Problème & Solution', link: '/Vision-Probleme-et-Solution' },
      { text: 'Ambition Open-Source', link: '/Vision-Ambition-Open-Source' },
      { text: 'Positionnement', link: '/Vision-Positionnement-Concurrentiel' },
    ],
  },
  {
    text: 'Historique',
    collapsed: true,
    items: [
      { text: 'Sprint Summary', link: '/Sprint-Summary' },
      { text: 'Roadmap', link: '/Roadmap' },
      { text: 'Décisions (ADR)', link: '/Decisions-Log' },
    ],
  },
]
