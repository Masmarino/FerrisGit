// Storybook-only helpers for page-level stories and layout building blocks.
import { applicationConfig, componentWrapperDecorator, type Decorator } from '@storybook/angular-vite';
import { useEffect } from 'storybook/preview-api';
import { provideFerrisgitIcons } from '../register-icons';

// `provideFerrisgitIcons` returns `EnvironmentProviders`, which only fit in an `ApplicationConfig`.
export const withFerrisgitIcons = applicationConfig({ providers: [provideFerrisgitIcons()] });

/** Reproduces the app shell's content area (page padding and background). Use with `layout: 'fullscreen'`. */
export const inShellContentArea = componentWrapperDecorator(
  (story) => `<div style="box-sizing: border-box; min-height: 100vh; padding: 1rem; background: var(--bg-panel);">${story}</div>`,
);

/** A 375px frame whatever the Storybook viewport: `gbt-page-layout` answers to its own width (container query). */
export const atPhoneWidth = componentWrapperDecorator((story) => `<div style="max-width: 375px; margin: 0 auto;">${story}</div>`);

// Story dates relative to "now", so relative dates ("il y a 5 min") never drift into absolute ones.
export const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
export const hoursAgo = (hours: number) => minutesAgo(hours * 60);
export const daysAgo = (days: number) => minutesAgo(days * 24 * 60);

/** Renders the story in the dark theme whatever the system setting (`[data-theme='dark']`, which Gabarit and the app follow), and restores the page after. */
export const inDarkTheme: Decorator = (story) => {
  useEffect(() => {
    const root = document.documentElement;
    const previous = root.getAttribute('data-theme');
    root.setAttribute('data-theme', 'dark');
    return () => (previous === null ? root.removeAttribute('data-theme') : root.setAttribute('data-theme', previous));
  }, []);
  return story();
};
