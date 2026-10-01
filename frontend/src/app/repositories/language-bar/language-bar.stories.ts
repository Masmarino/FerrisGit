import type { Meta, StoryObj } from '@storybook/angular-vite';
import { componentWrapperDecorator, moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of } from 'rxjs';
import { LanguageBar } from './language-bar';
import { LanguageStat, RepositoriesService } from '../repositories.service';

function withLanguages(languages: LanguageStat[]) {
  return moduleMetadata({
    providers: [{ provide: RepositoriesService, useValue: { getLanguages: () => of({ languages }) } }],
  });
}

const meta: Meta<LanguageBar> = {
  title: 'Repositories/LanguageBar',
  component: LanguageBar,
  tags: ['autodocs'],
  args: { repositoryId: 'repo-1', ref: 'HEAD' },
  decorators: [componentWrapperDecorator((story) => `<div style="width: 300px; max-width: 100%;">${story}</div>`)],
};

export default meta;
type Story = StoryObj<LanguageBar>;

export const SingleLanguage: Story = {
  decorators: [withLanguages([{ name: 'Rust', bytes: 184_320, percentage: 100 }])],
};

export const SeveralLanguages: Story = {
  decorators: [
    withLanguages([
      { name: 'Rust', bytes: 412_000, percentage: 62.4 },
      { name: 'TypeScript', bytes: 168_000, percentage: 25.5 },
      { name: 'HTML', bytes: 46_000, percentage: 7 },
      { name: 'CSS', bytes: 24_000, percentage: 3.6 },
      { name: 'Other', bytes: 10_000, percentage: 1.5 },
    ]),
  ],
};

export const TinyPercentages: Story = {
  decorators: [
    withLanguages([
      { name: 'TypeScript', bytes: 980_000, percentage: 97.8 },
      { name: 'Shell', bytes: 12_000, percentage: 1.2 },
      { name: 'YAML', bytes: 6_000, percentage: 0.6 },
      { name: 'Markdown', bytes: 3_000, percentage: 0.3 },
      { name: 'SQL', bytes: 1_000, percentage: 0.1 },
    ]),
  ],
};

export const UnknownLanguageFallbackColor: Story = {
  decorators: [
    withLanguages([
      { name: 'Rust', bytes: 300_000, percentage: 75 },
      { name: 'Zig', bytes: 100_000, percentage: 25 },
    ]),
  ],
};

export const NoLanguages: Story = {
  decorators: [withLanguages([])],
};

export const Loading: Story = {
  decorators: [moduleMetadata({ providers: [{ provide: RepositoriesService, useValue: { getLanguages: () => NEVER } }] })],
};
