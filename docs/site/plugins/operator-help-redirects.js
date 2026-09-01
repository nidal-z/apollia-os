// Serve the old French operator-help URLs as redirects to the English routes.
//
// The English pages under `docs/operator-help` kept their French file names
// when their content was translated (the file name carries the git history and
// pairs the page with its mirror), and each declares an English `slug:` in its
// front matter instead. The URLs the site served before that change are the
// ones this file redirects: released application builds link five of them, and
// external bookmarks hold the rest.
//
// This is a local plugin rather than `@docusaurus/plugin-client-redirects`
// because the repository adds no dependency without a stated decision, and
// thirty lines of `postBuild` do not justify one. The emitted page is the same
// shape that plugin emits: a canonical link, a meta refresh and a script, so
// crawlers move over and browsers follow even with scripts off.
//
// Both locales get the redirect files, because the rename moved both routes.
// The French mirror declares the same `slug:` as its English source, which is
// what Docusaurus needs for the language switcher and the `hreflang` alternates
// to resolve: those are computed by swapping the locale prefix of the current
// path, so a page whose route differs between locales advertises a French URL
// that does not exist. Aligning the routes fixed fifty dead alternates and
// retired the fifty French URLs the mirror used to serve, so those are
// redirected here too, under `/fr/`, from the same table.

const fs = require('fs');
const path = require('path');

// Old default-locale route -> new route, one entry per page whose declared
// slug differs from its file path. `scripts/check_docs_routes.py` names this
// file as where an old URL goes on living.
const REDIRECTS = {
  '/operator-help/agents/installer-un-agent': '/operator-help/agents/install-an-agent',
  '/operator-help/agents/demarrer-un-agent': '/operator-help/agents/start-an-agent',
  '/operator-help/agents/choisir-un-palier-d-autonomie': '/operator-help/agents/choose-an-autonomy-level',
  '/operator-help/agents/consulter-les-logs-d-un-agent': '/operator-help/agents/view-an-agent-logs',
  '/operator-help/agents/mesurer-un-agent-avec-eval': '/operator-help/agents/measure-an-agent-with-eval',
  '/operator-help/automatisations/programmer-un-trigger': '/operator-help/automations/schedule-a-trigger',
  '/operator-help/automatisations/suivre-l-historique-d-un-trigger': '/operator-help/automations/track-a-trigger-history',
  '/operator-help/chat/discuter-avec-votre-ia': '/operator-help/chat/chat-with-your-ai',
  '/operator-help/chat/activer-la-dictee-vocale': '/operator-help/chat/enable-voice-dictation',
  '/operator-help/controle/approuver-ou-refuser-une-action': '/operator-help/control/approve-or-reject-an-action',
  '/operator-help/controle/configurer-les-permissions-de-fichiers': '/operator-help/control/manage-tool-permissions',
  '/operator-help/controle/inspecter-un-outil': '/operator-help/control/inspect-a-tool',
  '/operator-help/installation/installer-sur-macos': '/operator-help/installation/install-on-macos',
  '/operator-help/installation/installer-sur-windows': '/operator-help/installation/install-on-windows',
  '/operator-help/installation/installer-sur-linux': '/operator-help/installation/install-on-linux',
  '/operator-help/installation/configurer-votre-profil': '/operator-help/installation/set-up-your-profile',
  '/operator-help/installation/telecharger-des-modeles-locaux': '/operator-help/installation/download-local-models',
  '/operator-help/installation/connecter-un-modele-distant': '/operator-help/installation/connect-a-remote-model',
  '/operator-help/installation/configurer-le-routage-hybride': '/operator-help/installation/configure-hybrid-routing',
  '/operator-help/installation/mettre-a-jour-apollia': '/operator-help/installation/update-apollia',
  '/operator-help/integrations/vue-d-ensemble-integrations': '/operator-help/integrations/integrations-overview',
  '/operator-help/integrations/comprendre-la-portee-d-une-integration': '/operator-help/integrations/understand-integration-scope',
  '/operator-help/integrations/connecter-google-workspace': '/operator-help/integrations/connect-google-workspace',
  '/operator-help/integrations/connecter-microsoft-365': '/operator-help/integrations/connect-microsoft-365',
  '/operator-help/integrations/connecter-un-serveur-mcp': '/operator-help/integrations/connect-an-mcp-server',
  '/operator-help/integrations/tester-une-connexion-mcp': '/operator-help/integrations/test-an-mcp-connection',
  '/operator-help/integrations/comprendre-les-permissions-mcp': '/operator-help/integrations/understand-mcp-permissions',
  '/operator-help/integrations/configurer-le-chargement-mcp-differe': '/operator-help/integrations/configure-deferred-mcp-loading',
  '/operator-help/integrations/cabler-son-propre-serveur-mcp': '/operator-help/integrations/wire-your-own-mcp-server',
  '/operator-help/integrations/gerer-les-tokens-oauth': '/operator-help/integrations/manage-oauth-tokens',
  '/operator-help/memoire/consulter-et-nettoyer-la-memoire': '/operator-help/memory/review-and-clean-up-memory',
  '/operator-help/memoire/gerer-mon-profil': '/operator-help/memory/manage-my-profile',
  '/operator-help/notifications/configurer-un-canal': '/operator-help/notifications/set-up-a-channel',
  '/operator-help/observabilite/consulter-l-audit-trail': '/operator-help/observability/read-the-audit-trail',
  '/operator-help/observabilite/consulter-l-historique-des-taches': '/operator-help/observability/read-the-activity-timeline',
  '/operator-help/observabilite/surveiller-les-couts-llm': '/operator-help/observability/monitor-ai-costs',
  '/operator-help/projets/creer-un-projet': '/operator-help/projects/create-a-project',
  '/operator-help/projets/lier-un-projet-a-un-chat': '/operator-help/projects/link-a-project-to-a-chat',
  '/operator-help/projets/activer-les-context-providers': '/operator-help/projects/enable-context-providers',
  '/operator-help/transversal/activer-la-compagnonne-ia': '/operator-help/transversal/enable-apollia-help',
  '/operator-help/transversal/naviguer-au-clavier-command-palette': '/operator-help/transversal/navigate-with-the-keyboard',
  '/operator-help/transversal/suivre-la-visite-guidee': '/operator-help/transversal/take-the-guided-tour',
  '/operator-help/transversal/trouver-sa-version-et-ses-donnees': '/operator-help/transversal/find-your-version-and-data',
  '/operator-help/transversal/utiliser-l-inbox': '/operator-help/transversal/use-the-inbox',
  '/operator-help/troubleshooting/la-dictee-vocale-ne-transcrit-rien': '/operator-help/troubleshooting/voice-dictation-transcribes-nothing',
  '/operator-help/troubleshooting/le-fournisseur-d-ia-ne-repond-pas': '/operator-help/troubleshooting/the-ai-provider-does-not-answer',
  '/operator-help/troubleshooting/le-runner-ne-demarre-pas': '/operator-help/troubleshooting/the-runner-does-not-start',
  '/operator-help/troubleshooting/un-agent-est-bloque': '/operator-help/troubleshooting/an-agent-is-stuck',
  '/operator-help/troubleshooting/une-action-est-refusee': '/operator-help/troubleshooting/an-action-was-refused',
  '/operator-help/troubleshooting/reinitialiser-apollia-factory-reset': '/operator-help/troubleshooting/factory-reset',
};

function redirectHtml(toUrl, locale) {
  return [
    '<!DOCTYPE html>',
    `<html lang="${locale}">`,
    '<head>',
    '<meta charset="UTF-8">',
    `<meta http-equiv="refresh" content="0; url=${toUrl}">`,
    `<link rel="canonical" href="${toUrl}">`,
    '</head>',
    `<script>window.location.href = ${JSON.stringify(toUrl)} + window.location.search + window.location.hash;</script>`,
    '</html>',
    '',
  ].join('\n');
}

module.exports = function operatorHelpRedirects(context) {
  return {
    name: 'operator-help-redirects',
    async postBuild({outDir, routesPaths}) {
      // A localized build carries its own baseUrl (`/fr/`) and writes into its
      // own outDir (`build/fr`), so the routes it reports are prefixed while
      // the files are not. The table below is written once, without a prefix,
      // and read against the prefixed routes.
      const locale = context.i18n.currentLocale;
      const prefix = context.baseUrl.replace(/\/$/, '');
      const routes = new Set(routesPaths);
      for (const [from, to] of Object.entries(REDIRECTS)) {
        const fromRoute = `${prefix}${from}`;
        const toRoute = `${prefix}${to}`;
        if (!routes.has(toRoute) && !routes.has(`${toRoute}/`)) {
          throw new Error(
            `operator-help-redirects: target ${toRoute} is not a built route`,
          );
        }
        if (routes.has(fromRoute) || routes.has(`${fromRoute}/`)) {
          throw new Error(
            `operator-help-redirects: ${fromRoute} is still a real route, a ` +
              'redirect would shadow it',
          );
        }
        const dir = path.join(outDir, ...from.split('/').filter(Boolean));
        fs.mkdirSync(dir, {recursive: true});
        fs.writeFileSync(
          path.join(dir, 'index.html'),
          redirectHtml(toRoute, locale),
        );
      }
    },
  };
};
