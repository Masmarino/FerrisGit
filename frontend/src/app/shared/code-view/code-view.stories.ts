import type { Meta, StoryObj } from '@storybook/angular-vite';
import { componentWrapperDecorator } from '@storybook/angular-vite';
import { CodeView } from './code-view';

const meta: Meta<CodeView> = {
  title: 'Shared/CodeView',
  component: CodeView,
  tags: ['autodocs'],
};

export default meta;
type Story = StoryObj<CodeView>;

export const Rust: Story = {
  args: {
    fileName: 'main.rs',
    content: `fn main() {\n    println!("Hello, FerrisGit!");\n}\n`,
  },
};

export const TypeScript: Story = {
  args: {
    fileName: 'initials-avatar.ts',
    content: `export function initialsFor(name: string): string {\n  return name.slice(0, 2).toUpperCase();\n}\n`,
  },
};

export const UnknownExtension: Story = {
  args: {
    fileName: 'Makefile',
    content: `build:\n\tcargo build --workspace\n`,
  },
};

export const WithLineNumbers: Story = {
  args: {
    fileName: 'initials-avatar.ts',
    lineNumbers: true,
    content: `/** Up to two initials from a display name ("Florian Simon" → "FS"). */\nexport function initialsFor(name: string): string {\n  const words = name.trim().split(/\\s+/).filter(Boolean);\n  if (words.length === 0) return '?';\n  const first = words[0][0];\n  const last = words.length > 1 ? words[words.length - 1][0] : '';\n  return (first + last).toUpperCase();\n}\n`,
  },
};

export const WithLineNumbersLongLines: Story = {
  decorators: [componentWrapperDecorator((story) => `<div style="max-width: 640px; border: 1px solid var(--border-color);">${story}</div>`)],
  args: {
    fileName: 'dev.sh',
    lineNumbers: true,
    content: Array.from({ length: 120 }, (_, i) =>
      i % 9 === 0
        ? `echo "Étape ${i + 1} : démarrage de l'API, de l'interface et de la base PostgreSQL de développement avec les migrations appliquées automatiquement au démarrage"`
        : `cargo run -p ferrisgit-api --quiet # étape ${i + 1}`,
    ).join('\n'),
  },
};
