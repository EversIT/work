import { defineConfig } from 'vitepress';

// The pages are the markdown files in `src/`.
//   DOCS_BASE      path the site is served under (default `/`; GitHub Pages project sites need `/<repo>/`)
//   DOCS_DEMO_URL  address of a demo (the frontend image started with DEMO_MODE=true); without it the
//                  navigation has no "Live demo" link
const base = process.env.DOCS_BASE ?? '/';
const demo = process.env.DOCS_DEMO_URL ?? '';

export default defineConfig({
  title: 'work',
  description:
    'Self-hosted work management: projects, boards, sprints, automations and a documented API.',
  lang: 'en-US',
  base,
  srcDir: './src',
  outDir: '../../dist/apps/docs',
  cleanUrls: true,
  lastUpdated: true,
  head: [
    ['link', { rel: 'icon', href: `${base}logo.svg`, type: 'image/svg+xml' }],
  ],
  themeConfig: {
    logo: '/logo.svg',
    nav: [
      { text: 'Install', link: '/install' },
      { text: 'Reference', link: '/CONFIGURATION' },
      ...(demo ? [{ text: 'Live demo', link: demo, target: '_blank' }] : []),
      { text: 'Changelog', link: 'https://github.com/EversIT/work/releases' },
    ],
    sidebar: [
      {
        text: 'Get started',
        items: [
          { text: 'Install with Docker', link: '/install' },
          { text: 'Upgrade and backup', link: '/operate' },
          { text: 'People and access', link: '/access' },
          { text: 'Using the API', link: '/api' },
        ],
      },
      {
        text: 'Reference',
        items: [
          { text: 'Configuration', link: '/CONFIGURATION' },
          { text: 'Security model', link: '/SECURITY-MODEL' },
          { text: 'Data migration', link: '/DATA-MIGRATION' },
        ],
      },
      {
        text: 'Contributing',
        items: [
          { text: 'Development', link: '/DEVELOPMENT' },
          { text: 'Architecture', link: '/ARCHITECTURE' },
        ],
      },
    ],
    socialLinks: [{ icon: 'github', link: 'https://github.com/EversIT/work' }],
    editLink: {
      pattern: 'https://github.com/EversIT/work/edit/main/apps/docs/src/:path',
      text: 'Edit this page on GitHub',
    },
    search: { provider: 'local' },
    footer: { message: 'Released under the MIT license.' },
  },
});
