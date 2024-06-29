module.exports = {
  title: 'Frontier',
  description: 'Ethereum compatibility layer for Substrate',
  base: '/frontier/',

  head: [
    ['meta', { name: 'theme-color', content: '#3eaf7c' }],
    ['meta', { name: 'apple-mobile-web-app-capable', content: 'yes' }],
    ['meta', { name: 'apple-mobile-web-app-status-bar-style', content: 'black' }]
  ],

  themeConfig: {
    repo: 'https://github.com/paritytech/frontier',
    editLinks: false,
    docsDir: 'docs',
    editLinkText: '',
    lastUpdated: false,
    nav: [
      { text: 'API reference', link: 'https://paritytech.github.io/frontier/rustdocs/pallet_evm' }
    ],
    sidebar: [
      'overview',
      'frame/evm',
      'frame/ethereum'
    ]
  },

  plugins: [
    '@vuepress/plugin-back-to-top',
    '@vuepress/plugin-medium-zoom',
  ]
}
