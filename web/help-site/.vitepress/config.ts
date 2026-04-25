import { defineConfig } from 'vitepress'
import { fileURLToPath, URL } from 'node:url'
import { sidebar } from './sidebar'

const BOOK_URL = process.env.BOOK_URL || 'https://book.apollia.fr'
const DOCS_URL = process.env.DOCS_URL || 'https://docs.apollia.fr'

export default defineConfig({
  title: 'Apollia — Centre d\'aide',
  description: 'Comment faire dans Apollia : guides pas-à-pas pour utiliser l\'application au quotidien.',
  lang: 'fr-FR',

  srcDir: 'content',

  // Cross-site links (book, wiki) + tolérance sur les liens inter-dossiers
  // (pendant la phase de contenu, à durcir plus tard).
  ignoreDeadLinks: true,

  cleanUrls: true,
  metaChunk: true,

  head: [
    ['link', { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' }],
    ['meta', { name: 'theme-color', content: '#5b8def' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:locale', content: 'fr_FR' }],
    ['meta', { property: 'og:site_name', content: 'Apollia · centre d\'aide' }],
  ],

  themeConfig: {
    siteTitle: 'Apollia · aide',
    nav: [
      { text: 'Accueil', link: '/' },
      { text: 'Apprendre (builder)', link: BOOK_URL },
      { text: 'Référence (docs)', link: DOCS_URL },
      { text: 'GitHub', link: 'https://github.com/nidal-z/apollia-os' },
    ],
    sidebar,
    outline: { level: [2, 3], label: 'Dans cette page' },
    docFooter: { prev: 'Précédent', next: 'Suivant' },
    lastUpdated: { text: 'Mis à jour le' },
    search: { provider: 'local' },
    editLink: {
      pattern: 'https://github.com/nidal-z/apollia-os/edit/main/help/:path',
      text: 'Améliorer cette page',
    },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/nidal-z/apollia-os' },
    ],
    footer: {
      message: 'Centre d\'aide opérateur d\'Apollia OS.',
      copyright: '© 2026 Apollia',
    },
    darkModeSwitchLabel: 'Apparence',
    sidebarMenuLabel: 'Menu',
    returnToTopLabel: 'Retour en haut',
    langMenuLabel: 'Langue',
  },

  markdown: {
    lineNumbers: false,
    html: false,
    theme: { light: 'github-light', dark: 'github-dark' },
    container: {
      tipLabel: 'Astuce',
      warningLabel: 'Attention',
      dangerLabel: 'Important',
      infoLabel: 'Info',
      detailsLabel: 'Détails',
    },
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
    resolve: {
      // Garder les symlinks = VitePress lit content/* comme des fichiers locaux
      // au lieu de résoudre vers les paths absolus help/*.
      preserveSymlinks: true,
    },
    ssr: {
      noExternal: ['vue', '@vue/server-renderer'],
    },
  },
})
