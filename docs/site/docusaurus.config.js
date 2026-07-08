// @ts-check
// Docusaurus site configuration for the Apollia OS adopters documentation.
// The three machine references (HTTP API, CLI, SDK) are generated from the
// source of truth, never hand-written. See regen.sh.

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Apollia OS',
  tagline: 'Sovereign runtime for autonomous AI agents',
  favicon: undefined,

  // Placeholders. Set the real domain when the deploy pipeline lands.
  url: 'https://docs.apollia.fr',
  baseUrl: '/',

  organizationName: 'Apollia-OS',
  projectName: 'apollia-os',

  // Placeholder pages cross-link before their targets are migrated, so broken
  // links warn rather than fail the build during phase 1.
  onBrokenLinks: 'warn',

  // `.md` parses as CommonMark, `.mdx` as MDX. This protects the generated CLI
  // reference (which contains <ARG> placeholders) from being read as JSX, while
  // the OpenAPI plugin's `.mdx` output keeps full MDX.
  markdown: {
    format: 'detect',
    hooks: {
      onBrokenMarkdownLinks: 'warn',
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

  themes: ['docusaurus-theme-openapi-docs'],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      navbar: {
        title: 'Apollia OS',
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
        style: 'dark',
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
        ],
        copyright: 'Apollia OS.',
      },
      prism: {
        additionalLanguages: ['bash', 'toml', 'python', 'rust', 'json'],
      },
    }),
};

module.exports = config;
