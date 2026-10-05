import type { StorybookConfig } from '@storybook/angular-vite';

const config: StorybookConfig = {
  stories: ['../src/app/**/*.stories.ts'],
  addons: ['@storybook/addon-a11y'],
  // Gabarit's fonts, where the app serves them (an asset of the build, which Storybook doesn't copy).
  staticDirs: [{ from: '../node_modules/@masmarino/gabarit/fonts', to: '/fonts' }],
  framework: {
    name: '@storybook/angular-vite',
    options: { compodoc: false },
  },
};

export default config;
