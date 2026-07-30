import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Stationeers Toolkit',
  description: 'Offline IC10 tooling, simulation, debugging, and Stationeers mod integration for VS Code.',
  base: '/stationeers-vscode-extension/',
  appearance: 'force-dark',
  // Some deep engineering pages intentionally link to repository files outside
  // the published docs tree.
  ignoreDeadLinks: true,
  head: [
    ['link', { rel: 'icon', type: 'image/png', href: '/stationeers-vscode-extension/icon.png' }],
  ],
  themeConfig: {
    logo: '/icon.png',
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'IC10 Reference', link: '/reference/ic10' },
      { text: 'Examples', link: '/examples/templates' },
      {
        text: 'Project',
        items: [
          { text: 'GitHub repository', link: 'https://github.com/ShaneYu/stationeers-vscode-extension' },
          { text: 'Releases', link: 'https://github.com/ShaneYu/stationeers-vscode-extension/releases' },
          { text: 'Report an issue', link: 'https://github.com/ShaneYu/stationeers-vscode-extension/issues' },
        ],
      },
    ],
    sidebar: {
      '/guide/': [
        { text: 'Start here', items: [
          { text: 'Getting started', link: '/guide/getting-started' },
          { text: 'What the toolkit does', link: '/guide/overview' },
          { text: 'Installation and updates', link: '/guide/installation' },
        ] },
        { text: 'Build and edit', items: [
          { text: 'IC10 editing', link: '/guide/ic10-editing' },
          { text: 'Deployment builds', link: '/guide/deployment-builds' },
          { text: 'Scenario testing', link: '/guide/scenario-testing' },
          { text: 'Debugging', link: '/guide/debugging' },
        ] },
        { text: 'Run and integrate', items: [
          { text: 'Simulation and debugging', link: '/guide/simulation' },
          { text: 'StationeersLua integration', link: '/guide/stationeers-lua' },
          { text: 'Stationeers Toolkit mod', link: '/guide/toolkit-mod' },
        ] },
        { text: 'Reference material', items: [
          { text: 'Commands and settings', link: '/guide/commands-settings' },
          { text: 'Troubleshooting', link: '/guide/troubleshooting' },
        ] },
      ],
      '/reference/': [
        { text: 'Language reference', items: [
          { text: 'IC10 language support', link: '/reference/ic10' },
          { text: 'Environment format', link: '/reference/environment-format' },
          { text: 'Scenario test format', link: '/reference/scenario-format' },
        ] },
      ],
      '/examples/': [
        { text: 'Starter projects', items: [
          { text: 'Template catalogue', link: '/examples/templates' },
          { text: 'One-door airlock', link: '/examples/airlock' },
          { text: 'Multi-IC networks', link: '/examples/networks' },
          { text: 'Scenario workbench', link: '/examples/workbench' },
        ] },
      ],
    },
    search: { provider: 'local' },
    editLink: {
      pattern: 'https://github.com/ShaneYu/stationeers-vscode-extension/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
    footer: {
      message: '<strong>⚠️ This documentation was AI-generated and may contain inaccuracies. Please submit pull requests with corrections as needed.</strong>',
      copyright: 'Stationeers is developed by RocketWerkz.',
    },
    outline: { level: [2, 3] },
  },
  markdown: {
    lineNumbers: true,
    // IC10 is structurally close enough to MIPS assembly to provide useful
    // highlighting until a dedicated Stationeers grammar is added.
    languageAlias: { ic10: 'mips' },
  },
})
