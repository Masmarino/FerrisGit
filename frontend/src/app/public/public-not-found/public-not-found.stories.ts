import type { Meta, StoryObj } from '@storybook/angular-vite';
import { PublicNotFound } from './public-not-found';
import { atPhoneWidth, inDarkTheme, inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { withPublicCatalog } from '../public-story-fixtures';

const meta: Meta<PublicNotFound> = {
  title: 'Public/PublicNotFound',
  component: PublicNotFound,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withFerrisgitIcons, withPublicCatalog({ url: '/repositories/acme/secret' }), inShellContentArea],
};

export default meta;
type Story = StoryObj<PublicNotFound>;

export const Default: Story = {};

export const Dark: Story = {
  decorators: [inDarkTheme],
};

export const Phone: Story = {
  decorators: [atPhoneWidth],
};
