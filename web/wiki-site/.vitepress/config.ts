import { defineConfig } from 'vitepress'
import { fileURLToPath, URL } from 'node:url'
import { sidebar } from './sidebar'

// URLs configurables par env var pour liens cross-site (book + help).
const BOOK_URL = process.env.BOOK_URL || 'https://book.apollia.fr'
const HELP_URL = process.env.HELP_URL || 'https://guide.apollia.fr'

export default defineConfig({
  title: 'Apollia OS — Référence technique',
  description: 'Référence consultée : briques, API, configurations, codes d\'erreur.',
  lang: 'fr-FR',

  // Source via symlinks dans content/ (évite les problèmes de résolution
  // SSR quand la source est hors du projet — cf. preserveSymlinks ci-dessous).
  srcDir: 'content',

  // Pages à ignorer (GitHub Wiki internals)
  srcExclude: ['**/_Sidebar.md', '**/_Footer.md'],

  // Cross-site links (book, help) — tolérance pendant la phase de contenu.
  ignoreDeadLinks: true,

  // Build cleaner
  cleanUrls: true,
  metaChunk: true,

  // SEO + accessibilité
  head: [
    ['link', { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' }],
    ['meta', { name: 'theme-color', content: '#5b8def' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:locale', content: 'fr_FR' }],
    ['meta', { property: 'og:site_name', content: 'Apollia Docs' }],
  ],

  themeConfig: {
    siteTitle: 'Apollia · docs',
    nav: [
      { text: 'Apprendre', link: BOOK_URL },
      { text: 'Référence', link: '/Home' },
      { text: 'Aide opérateur', link: HELP_URL },
      { text: 'GitHub', link: 'https://github.com/nidal-z/apollia-os' },
    ],
    sidebar,
    outline: { level: [2, 3], label: 'Sur cette page' },
    docFooter: { prev: 'Précédent', next: 'Suivant' },
    lastUpdated: { text: 'Dernière mise à jour' },
    search: { provider: 'local' },
    editLink: {
      pattern: 'https://github.com/nidal-z/apollia-os/edit/main/docs/wiki/:path',
      text: 'Modifier cette page sur GitHub',
    },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/nidal-z/apollia-os' },
    ],
    footer: {
      message: 'Référence technique d\'Apollia OS · runtime open-source pour agents IA autonomes.',
      copyright: '© 2026 Apollia',
    },
    darkModeSwitchLabel: 'Apparence',
    sidebarMenuLabel: 'Menu',
    returnToTopLabel: 'Retour en haut',
    langMenuLabel: 'Langue',
  },

  markdown: {
    lineNumbers: false,
    // Désactive l'HTML brut inline (les `<Vec<T>>` Rust sont alors escapés).
    // Les tags HTML explicites (ex: <br>) dans le markdown source ne marchent
    // plus — acceptable pour une référence technique.
    html: false,
    theme: { light: 'github-light', dark: 'github-dark' },
    container: {
      tipLabel: 'Astuce',
      warningLabel: 'Attention',
      dangerLabel: 'Danger',
      infoLabel: 'Info',
      detailsLabel: 'Détails',
    },
    // Échappe `{{` et `}}` en entités HTML avant que Vue SFC ne les
    // interprète comme interpolations. Les pages d'Apollia contiennent
    // des templates de pipelines (`{{steps.x.output}}`) et des exemples
    // TOML/Rust qui utilisent ces séquences.
    config(md) {
      md.core.ruler.before('normalize', 'escape-vue-interpolation', (state) => {
        state.src = state.src
          .replace(/\{\{/g, '&#123;&#123;')
          .replace(/\}\}/g, '&#125;&#125;')
      })
      const origFence = md.renderer.rules.fence
      if (origFence) {
        md.renderer.rules.fence = (tokens, idx, options, env, self) => {
          const rendered = origFence(tokens, idx, options, env, self)
          return rendered.replace(/<pre(?![^>]*v-pre)/g, '<pre v-pre')
        }
      }
    },
  },

  vite: {
    resolve: { preserveSymlinks: true },
    ssr: { noExternal: ['vue', '@vue/server-renderer'] },
  },
})
