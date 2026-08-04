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

  // This host is not just where the site is published: the desktop binary now
  // links into it from About and from Help, so it is a runtime dependency of
  // shipped screens, not only of CI. It still needs the org-side setup the
  // Pages job records (Pages environment active, custom domain bound). Until
  // that is confirmed live, those in-app links resolve to nothing, which is the
  // same dead-link failure the retired repository wiki produced. Nothing
  // catches it here: lychee runs `--offline` and never resolves the host.
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
      fr: { label: 'Francais' },
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
  ],

  themes: ['docusaurus-theme-openapi-docs', '@docusaurus/theme-mermaid'],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
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
        items: [
          { to: '/', label: 'Docs', position: 'left' },
          { to: '/reference', label: 'Reference', position: 'left' },
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
