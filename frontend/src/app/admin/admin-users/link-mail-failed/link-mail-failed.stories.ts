import type { Meta, StoryObj } from '@storybook/angular-vite';
import { componentWrapperDecorator } from '@storybook/angular-vite';
import { LinkMailFailed } from './link-mail-failed';
import { withFerrisgitIcons } from '../../../shared/layout/page-story-helpers';

const meta: Meta<LinkMailFailed> = {
  title: 'Admin/LinkMailFailed',
  component: LinkMailFailed,
  tags: ['autodocs'],
  decorators: [
    withFerrisgitIcons,
    componentWrapperDecorator((story) => `<div style="box-sizing: border-box; max-width: 30rem; padding: 1rem; background: var(--bg-principal);">${story}</div>`),
  ],
  args: {
    username: 'dave',
    emailError: 'connection refused (smtp.mailgun.org:587)',
    url: 'https://ferrisgit.example.com/activate#token=3f9c1a7be25d4c8e9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5',
  },
};

export default meta;
type Story = StoryObj<LinkMailFailed>;

export const WithReason: Story = {};

export const WithoutReason: Story = { args: { emailError: undefined } };

export const Dismissible: Story = { args: { dismissible: true } };

export const WithoutLink: Story = { args: { url: undefined } };

export const PasswordReset: Story = {
  args: { kind: 'password-reset', dismissible: true, url: 'https://ferrisgit.example.com/reset-password#token=3f9c1a7be25d4c8e9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5' },
};
