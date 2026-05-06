// Sidebar du centre d'aide.
// Ordre intentionnel : workflows quotidiens d'abord, troubleshooting en bas.
// cleanUrls actif → pas d'extension dans les liens.

import type { DefaultTheme } from 'vitepress'

export const sidebar: DefaultTheme.Sidebar = [
  {
    text: 'Installation',
    collapsed: false,
    items: [
      { text: 'Configurer votre profil', link: '/installation/configurer-votre-profil' },
      { text: 'Connecter un fournisseur d\'IA', link: '/installation/connecter-un-fournisseur-d-ia' },
      { text: 'Télécharger des modèles locaux', link: '/installation/telecharger-des-modeles-locaux' },
    ],
  },
  {
    text: 'Discuter',
    collapsed: false,
    items: [
      { text: 'Discuter avec votre IA', link: '/chat/discuter-avec-votre-ia' },
      { text: 'Activer la dictée vocale', link: '/chat/activer-la-dictee-vocale' },
    ],
  },
  {
    text: 'Agents',
    collapsed: false,
    items: [
      { text: 'Installer un agent', link: '/agents/installer-un-agent' },
      { text: 'Démarrer un agent', link: '/agents/demarrer-un-agent' },
      { text: 'Consulter les logs d\'un agent', link: '/agents/consulter-les-logs-d-un-agent' },
    ],
  },
  {
    text: 'Projets',
    collapsed: false,
    items: [
      { text: 'Créer un projet', link: '/projets/creer-un-projet' },
      { text: 'Activer les context providers', link: '/projets/activer-les-context-providers' },
      { text: 'Lier un projet à un chat', link: '/projets/lier-un-projet-a-un-chat' },
    ],
  },
  {
    text: 'Mémoire',
    collapsed: false,
    items: [
      { text: 'Consulter et nettoyer la mémoire', link: '/memoire/consulter-et-nettoyer-la-memoire' },
      { text: 'Configurer la rétention de mémoire', link: '/memoire/configurer-la-retention-de-memoire' },
    ],
  },
  {
    text: 'Automatisations',
    collapsed: false,
    items: [
      { text: 'Programmer un trigger', link: '/automatisations/programmer-un-trigger' },
      { text: 'Suivre l\'historique d\'un trigger', link: '/automatisations/suivre-l-historique-d-un-trigger' },
    ],
  },
  {
    text: 'Contrôle',
    collapsed: false,
    items: [
      { text: 'Approuver ou refuser une action', link: '/controle/approuver-ou-refuser-une-action' },
      { text: 'Configurer les permissions', link: '/controle/configurer-les-permissions-de-fichiers' },
    ],
  },
  {
    text: 'Intégrations',
    collapsed: false,
    items: [
      { text: 'Connecter un serveur MCP', link: '/integrations/connecter-un-serveur-mcp' },
      { text: 'Tester une connexion MCP', link: '/integrations/tester-une-connexion-mcp' },
    ],
  },
  {
    text: 'Observabilité',
    collapsed: false,
    items: [
      { text: 'Lire le digest quotidien', link: '/observabilite/lire-le-digest-quotidien' },
      { text: 'Surveiller les coûts d\'IA', link: '/observabilite/surveiller-les-couts-llm' },
      { text: 'Consulter l\'audit trail', link: '/observabilite/consulter-l-audit-trail' },
      { text: 'Consulter l\'historique des tâches', link: '/observabilite/consulter-l-historique-des-taches' },
    ],
  },
  {
    text: 'Notifications',
    collapsed: true,
    items: [
      { text: 'Configurer un canal', link: '/notifications/configurer-un-canal' },
      { text: 'Choisir les événements notifiés', link: '/notifications/choisir-les-evenements-notifies' },
    ],
  },
  {
    text: 'Transversal',
    collapsed: true,
    items: [
      { text: 'Utiliser l\'inbox', link: '/transversal/utiliser-l-inbox' },
      { text: 'Activer la compagnonne IA', link: '/transversal/activer-la-compagnonne-ia' },
      { text: 'Naviguer au clavier', link: '/transversal/naviguer-au-clavier-command-palette' },
    ],
  },
  {
    text: 'Si ça coince',
    collapsed: false,
    items: [
      { text: 'Le fournisseur d\'IA ne répond pas', link: '/troubleshooting/le-fournisseur-d-ia-ne-repond-pas' },
      { text: 'Un agent est bloqué', link: '/troubleshooting/un-agent-est-bloque' },
      { text: 'Une action a été refusée', link: '/troubleshooting/une-action-est-refusee' },
      { text: 'La dictée ne transcrit rien', link: '/troubleshooting/la-dictee-vocale-ne-transcrit-rien' },
      { text: 'Réinitialiser Apollia', link: '/troubleshooting/reinitialiser-apollia-factory-reset' },
    ],
  },
]
