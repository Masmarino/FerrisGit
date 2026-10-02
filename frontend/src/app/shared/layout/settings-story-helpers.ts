// Storybook-only helpers for the settings sections.
import { componentWrapperDecorator } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';

/** A settings section as the page lays it out: the width of the main column beside the nav (~920px). */
export const inSettingsColumn = componentWrapperDecorator(
  (story) =>
    `<div style="box-sizing: border-box; min-height: 100vh; padding: 1rem; background: var(--bg-panel);"><div style="max-width: 920px;">${story}</div></div>`,
);

export const fakeToast = { show: () => {}, dismiss: () => {} };

export async function rendered<T extends Element = HTMLElement>(canvasElement: HTMLElement, selector: string): Promise<T> {
  return waitFor(() => {
    const element = canvasElement.querySelector<T>(selector);
    if (!element) {
      throw new Error(`${selector} not rendered yet`);
    }
    return element;
  });
}

/**
 * Layout checks jsdom can't do: no horizontal overflow, cards stacked and as wide as the column, every icon
 * registered, a 44px retry target on a phone.
 */
export async function expectSettingsLayout(canvasElement: HTMLElement): Promise<void> {
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden frame (docs page): nothing to measure
  }
  await expect(doc.scrollWidth, 'page scroll width').toBeLessThanOrEqual(doc.clientWidth);

  if (doc.clientWidth <= 480) {
    for (const button of Array.from(canvasElement.querySelectorAll<HTMLElement>('.gbt-alert__actions .gbt-button'))) {
      await expect(button.getBoundingClientRect().height, `"${button.textContent?.trim()}" is a 44px target`).toBeGreaterThanOrEqual(43.5);
    }
  }

  const cards = Array.from(canvasElement.querySelectorAll<HTMLElement>('gbt-card'));
  let previousBottom = -Infinity;
  for (const card of cards) {
    const box = card.getBoundingClientRect();
    const name = card.querySelector('h2')?.textContent?.trim();
    await expect(box.top, `"${name}" below the previous card`).toBeGreaterThanOrEqual(previousBottom);
    const parentWidth = card.parentElement!.getBoundingClientRect().width;
    await expect(Math.round(box.width), `"${name}" spans its column`).toBe(Math.round(parentWidth));
    const header = card.querySelector<HTMLElement>(':scope > .gbt-card__header')!;
    const inner = card.querySelector<HTMLElement>(':scope > .gbt-card')!;
    await expect(header.getBoundingClientRect().bottom, `"${name}" heading above its box`).toBeLessThanOrEqual(inner.getBoundingClientRect().top + 0.5);
    // Nothing sticks out of the card, be it a field, a row or a chip.
    for (const child of Array.from(card.querySelectorAll<HTMLElement>('.gbt-card__body *'))) {
      const childBox = child.getBoundingClientRect();
      if (childBox.width === 0 || getComputedStyle(child).position === 'fixed' || child.closest('.sr-only')) {
        continue;
      }
      await expect(childBox.right, `${child.tagName.toLowerCase()}.${child.className} inside "${name}"`).toBeLessThanOrEqual(box.right + 0.5);
    }
    previousBottom = box.bottom;
  }
  for (const icon of Array.from(canvasElement.querySelectorAll('gbt-icon'))) {
    if (icon.getBoundingClientRect().width > 0) {
      await expect(icon.querySelector('svg'), `icon in "${icon.parentElement?.textContent?.trim()}"`).not.toBeNull();
    }
  }
}

/** Rows: one truncated line per title, trailing actions on one right edge, 32px icon-only actions. */
export async function expectRows(canvasElement: HTMLElement): Promise<void> {
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return;
  }
  const rows = Array.from(canvasElement.querySelectorAll<HTMLElement>('gbt-list-row'));
  const rights = new Set<number>();
  for (const row of rows) {
    const trailing = row.querySelector<HTMLElement>('.gbt-list-row__trailing');
    if (trailing && trailing.getBoundingClientRect().width > 0) {
      rights.add(Math.round(trailing.getBoundingClientRect().right));
    }
    for (const button of Array.from(row.querySelectorAll<HTMLElement>('.gbt-list-row__trailing .gbt-button--icon-only'))) {
      const box = button.getBoundingClientRect();
      await expect(Math.round(box.width), 'icon button width').toBeGreaterThanOrEqual(32);
      await expect(Math.round(box.height), 'icon button height').toBeGreaterThanOrEqual(32);
    }
  }
  await expect(rights.size, 'trailing actions on one right edge').toBeLessThanOrEqual(1);
}
