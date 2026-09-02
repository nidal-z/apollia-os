// @ts-check
// Docusaurus site configuration for the Apollia OS adopters documentation.
// The three machine references (HTTP API, CLI, SDK) are generated from the
// source of truth, never hand-written. See regen.sh.

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Apollia OS',
  tagline: 'Sovereign runtime for autonomous AI agents',
  // The Apollia symbol, the same vector the desktop app serves from
  // ui/public/logo.svg. Source of truth: crates/apollia-desktop/icons/logo.svg.
  favicon: 'img/favicon.png',

  // This host is not just where the site is published: the desktop binary
  // links into it from About and from Help, so it is a runtime dependency of
  // shipped screens. Publication follows the same model as the showcase site:
  // the hosting provider builds and publishes the site itself on every push,
  // so no CI job in this repository deploys it. The console-side setup
  // (project created, domain bound) is a human gesture outside this tree.
  // Until it is confirmed live, those in-app links resolve to nothing, which
  // is the same dead-link failure the retired repository wiki produced.
  // Nothing catches it here: lychee runs `--offline` and never resolves the
  // host.
  url: 'https://docs.apollia.fr',
  baseUrl: '/',

  organizationName: 'Apollia-OS',
  projectName: 'apollia-os',

  // The corpus is migrated and the site is at zero broken links, so a broken
  // link fails the build (deploy gate) rather than warning.
  onBrokenLinks: 'throw',

  // `.md` parses as CommonMark, `.mdx` as MDX. This protects the generated CLI
  // reference (which contains <ARG> placeholders) from being read as JSX, while
  // the OpenAPI plugin's `.mdx` output keeps full MDX.
  markdown: {
    format: 'detect',
    // Enables ```mermaid fenced blocks, used by the arc42 architecture pages
    // for the C4 context, container, and runtime sequence diagrams. Build-time
    // only; the sovereign runtime is untouched.
    mermaid: true,
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'fr'],
    localeConfigs: {
      en: { label: 'English' },
      fr: { label: 'Français' },
    },
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.js',
          // Required by docusaurus-theme-openapi-docs to render API pages.
          docItemComponent: '@theme/ApiItem',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
        // The French sitemap declared 253 URLs, of which 143 served English:
        // the 125 generated API pages, the 17 SDK reference pages and the CLI
        // reference. Docusaurus creates a route per locale whether or not a
        // translation exists, and announced every one of them as French.
        //
        // They are dropped from the French sitemap rather than translated,
        // because all three are generated from the source of truth and the
        // generator writes English only: a hand-written French copy would go
        // stale on the next `regen.sh` with nothing to catch it. An API
        // signature has no language, and a URL announced as French that serves
        // English is a quality signal a search engine reads against the site.
        // The pages stay reachable and stay indexable under the default locale.
        //
        // No locale test guards these patterns, and none is needed: the `/fr/`
        // prefix only exists in the French render, so the same list matches
        // nothing in the English one. A test on DOCUSAURUS_CURRENT_LOCALE would
        // also have been wrong here, because this module is evaluated once and
        // reused for both locales.
        sitemap: {
          ignorePatterns: [
            '/fr/reference/api/**',
            '/fr/reference/sdk/**',
            '/fr/reference/cli/**',
          ],
        },
      }),
    ],
  ],

  plugins: [
    [
      'docusaurus-plugin-openapi-docs',
      {
        id: 'openapi',
        docsPluginId: 'classic',
        config: {
          runtime: {
            // Source of truth: the committed OpenAPI spec generated from code.
            specPath: '../../clients/openapi.json',
            outputDir: 'docs/reference/api',
            sidebarOptions: {
              groupPathsBy: 'tag',
              categoryLinkSource: 'tag',
            },
          },
        },
      },
    ],
    // The old French operator-help URLs, served as redirects to the English
    // routes the pages now declare. Local plugin, no dependency.
    './plugins/operator-help-redirects.js',
  ],

  // Structured data for the whole site, not only the home page. The search
  // action is real now that the site has a search page; declaring one without
  // it would have been a claim a crawler can check and find false.
  headTags: [
    {
      tagName: 'script',
      attributes: { type: 'application/ld+json' },
      innerHTML: JSON.stringify({
        '@context': 'https://schema.org',
        '@graph': [
          {
            '@type': 'Organization',
            '@id': 'https://apollia.fr/#organization',
            name: 'Apollia',
            url: 'https://apollia.fr',
            logo: 'https://docs.apollia.fr/img/logo.svg',
          },
          {
            '@type': 'WebSite',
            '@id': 'https://docs.apollia.fr/#website',
            name: 'Apollia OS documentation',
            url: 'https://docs.apollia.fr',
            publisher: { '@id': 'https://apollia.fr/#organization' },
            inLanguage: ['en', 'fr'],
            potentialAction: {
              '@type': 'SearchAction',
              target: {
                '@type': 'EntryPoint',
                urlTemplate: 'https://docs.apollia.fr/search?q={search_term_string}',
              },
              'query-input': 'required name=search_term_string',
            },
          },
        ],
      }),
    },
  ],

  themes: [
    'docusaurus-theme-openapi-docs',
    '@docusaurus/theme-mermaid',
    // Search, indexed at build time and served from this origin. The classic
    // preset ships none, so 252 pages had no search field at all, which is the
    // one defect that cancels the value of the other 251: nobody walks six
    // sections and twelve sub-sections to find how to configure a notification
    // channel.
    //
    // Local rather than Algolia DocSearch, and not only because DocSearch needs
    // an application and a wait. A product that sells sovereignty does not send
    // every reader's query to a third party to find its own pages, and this
    // site already serves its fonts from its own origin and carries no tracker.
    // The index is built into the bundle; nothing leaves the browser.
    //
    // Four of its components are swizzled into src/theme, because the plugin
    // writes `aria-label="Search"` as a literal on both of its inputs instead
    // of translating it. scripts/check_swizzled_theme.py holds those copies
    // against this package so the fork cannot drift in silence.
    [
      require.resolve('@easyops-cn/docusaurus-search-local'),
      {
        hashed: true,
        language: ['en', 'fr'],
        indexBlog: false,
        docsRouteBasePath: '/',
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
      },
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      // Every page shared on a chat or a social network drew an empty card:
      // no page carried an og:image, and the theme had no default. One image
      // in the charter covers all 252 at once; a page that wants its own still
      // sets `image:` in its frontmatter.
      image: 'img/social-card.png',
      colorMode: {
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: 'Apollia OS',
        // One artwork, seated on its own paper. The symbol is a
        // light-background mark: its blue swoosh measures 1.57:1 against the
        // dark theme's background, under the 3:1 floor WCAG 1.4.11 sets for a
        // non-text graphic, so on a dark page the body of the mark drops out.
        // There is no dark vector to switch `srcDark` to, so src/css/custom.css
        // seats it on `--logo-paper` instead, in both themes.
        logo: {
          alt: 'Apollia OS',
          src: 'img/logo.svg',
        },
        // The most frequent path into a documentation is a search engine, not
        // the showcase site, and this one used to be a dead end: a reader who
        // landed on a tutorial could understand the product and had nowhere to
        // go to install it. Nothing in the navbar or the footer left the
        // subdomain except GitHub.
        items: [
          { to: '/', label: 'Docs', position: 'left' },
          { to: '/reference', label: 'Reference', position: 'left' },
          // The help center used to be reachable only from the sidebar, in
          // last position, behind five technical sections. It is the entry
          // point of the audience least able to find it.
          { to: '/operator-help', label: 'Help center', position: 'left' },
          {
            href: 'https://apollia.fr',
            label: 'Product',
            position: 'right',
          },
          {
            href: 'https://apollia.fr/telecharger',
            label: 'Download',
            position: 'right',
            className: 'navbar__item navbar__link navbar-download',
          },
          {
            href: 'https://github.com/Apollia-OS/apollia-os',
            label: 'GitHub',
            position: 'right',
          },
          {
            type: 'localeDropdown',
            position: 'right',
          },
        ],
      },
      footer: {
        // `light` lets the footer inherit the charter surfaces from
        // src/css/custom.css instead of forcing Infima's near-black block,
        // which would fight the warm greige in the light theme.
        style: 'light',
        logo: {
          alt: 'Apollia OS',
          src: 'img/logo.svg',
          width: 36,
          height: 36,
        },
        links: [
          {
            title: 'Docs',
            items: [
              { label: 'Tutorials', to: '/tutorials' },
              { label: 'How-to guides', to: '/how-to' },
              { label: 'Reference', to: '/reference' },
              { label: 'Explanation', to: '/explanation' },
            ],
          },
          {
            title: 'Using Apollia',
            items: [
              { label: 'Help center', to: '/operator-help' },
              { label: 'Install the desktop app', to: '/how-to/install-the-desktop-app' },
              { label: 'Architecture', to: '/architecture' },
            ],
          },
          {
            title: 'Apollia',
            items: [
              { label: 'The product', href: 'https://apollia.fr' },
              { label: 'Download', href: 'https://apollia.fr/telecharger' },
              { label: 'For integrators', href: 'https://apollia.fr/entreprises' },
            ],
          },
          {
            title: 'Project',
            items: [
              { label: 'GitHub', href: 'https://github.com/Apollia-OS/apollia-os' },
              {
                label: 'Discussions',
                href: 'https://github.com/Apollia-OS/apollia-os/discussions',
              },
              {
                label: 'Report a problem',
                href: 'https://github.com/Apollia-OS/apollia-os/issues/new',
              },
            ],
          },
        ],
        copyright: 'Apollia OS. Licensed under MIT OR Apache-2.0.',
      },
      prism: {
        additionalLanguages: ['bash', 'toml', 'python', 'rust', 'json'],
      },
    }),
};

module.exports = config;
