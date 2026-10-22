import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const repo = 'https://github.com/usecornercase/cornercase-terminal';
const discord = 'https://discord.gg/aYqCa4xC4R';
const site = process.env.SITE_ORIGIN || 'https://usecornercase.dev';
const base = process.env.SITE_BASE ?? '';

export default defineConfig({
  site,
  base,
  trailingSlash: 'always',
  build: { inlineStylesheets: 'never' },
  vite: { build: { assetsInlineLimit: 0 } },
  integrations: [
    starlight({
      title: 'cornercase',
      description:
        'A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse.',
      logo: { light: './src/assets/logo-light.svg', dark: './src/assets/logo-dark.svg', alt: '' },
      favicon: '/favicon.svg',
      social: [
        { icon: 'github', label: 'GitHub', href: repo },
        { icon: 'discord', label: 'Discord', href: discord },
      ],
      editLink: { baseUrl: `${repo}/edit/main/site/` },
      expressiveCode: {
        themes: ['catppuccin-mocha', 'catppuccin-latte'],
        styleOverrides: {
          borderRadius: '0',
          codeFontFamily: "'JetBrains Mono Variable', ui-monospace, monospace",
          uiFontFamily: "'Archivo Variable', ui-sans-serif, system-ui, sans-serif",
          codeBackground: ({ theme }) => (theme.type === 'dark' ? '#111317' : '#f1f2f4'),
          frames: {
            editorTabBarBackground: ({ theme }) => (theme.type === 'dark' ? '#16181d' : '#e6e8eb'),
            terminalTitlebarBackground: ({ theme }) => (theme.type === 'dark' ? '#16181d' : '#e6e8eb'),
            terminalBackground: ({ theme }) => (theme.type === 'dark' ? '#111317' : '#f1f2f4'),
            frameBoxShadowCssValue: 'none',
          },
        },
      },
      customCss: [
        '@fontsource-variable/archivo/wdth.css',
        '@fontsource-variable/jetbrains-mono',
        './src/styles/symbols.css',
        './src/styles/docs.css',
      ],
      head: [
        { tag: 'meta', attrs: { property: 'og:image', content: `${site}${base}/og.png` } },
        { tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
      ],
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'Introduction', slug: 'docs' },
            { label: 'Installation', slug: 'docs/installation' },
            { label: 'First steps', slug: 'docs/first-steps' },
          ],
        },
        {
          label: 'Using cornercase',
          items: [
            { label: 'Projects, workspaces and tabs', slug: 'docs/guides/projects-workspaces-tabs' },
            { label: 'Panes and splits', slug: 'docs/guides/panes-and-splits' },
            { label: 'Git worktrees', slug: 'docs/guides/worktrees' },
            { label: 'Changes panel', slug: 'docs/guides/changes' },
            { label: 'Files panel', slug: 'docs/guides/files' },
            { label: 'TODO list', slug: 'docs/guides/todo' },
            { label: 'Issues and agents', slug: 'docs/guides/issues-and-agents' },
            { label: 'Flows', slug: 'docs/guides/flows' },
            { label: 'Community flows', link: '/flows/' },
            { label: 'Queue agents', slug: 'docs/guides/queue' },
            { label: 'Accounts and profiles', slug: 'docs/guides/accounts' },
            { label: 'Scripts and agents', slug: 'docs/guides/scripts-and-agents' },
            { label: 'Search', slug: 'docs/guides/search' },
            { label: 'Sessions and the server', slug: 'docs/guides/sessions' },
            { label: 'Remote machines', slug: 'docs/guides/remote' },
            { label: 'From your phone', slug: 'docs/guides/phone' },
            { label: 'On Telegram', slug: 'docs/guides/telegram' },
            { label: 'On Discord', slug: 'docs/guides/discord' },
            { label: 'Small terminals', slug: 'docs/guides/small-terminals' },
            { label: 'Plugins', slug: 'docs/guides/plugins' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Settings and config.json', slug: 'docs/reference/configuration' },
            { label: 'Agents', slug: 'docs/reference/agents' },
            { label: 'Prompt template', slug: 'docs/reference/prompt' },
            { label: 'Keyboard and mouse', slug: 'docs/reference/keyboard-and-mouse' },
            { label: 'Files and environment', slug: 'docs/reference/files-and-environment' },
            { label: 'Command line', slug: 'docs/reference/command-line' },
          ],
        },
        {
          label: 'Help',
          items: [
            { label: 'Troubleshooting', slug: 'docs/help/troubleshooting' },
            { label: 'FAQ', slug: 'docs/help/faq' },
            { label: 'Known limitations', slug: 'docs/help/limitations' },
          ],
        },
        {
          label: 'Contributing',
          items: [
            { label: 'Architecture', slug: 'docs/contributing/architecture' },
            { label: 'Development', slug: 'docs/contributing/development' },
            { label: 'Publish a flow', slug: 'docs/contributing/flows' },
          ],
        },
      ],
    }),
  ],
});
