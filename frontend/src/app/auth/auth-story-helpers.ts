// Storybook helpers for the public sign-in pages, which sit inside Gabarit's `gbt-auth-panel`. The app does not import them.
import { waitFor } from 'storybook/test';

const rect = (el: Element) => el.getBoundingClientRect();

const BLOCK = 'gbt-auth-panel';

export function assertPanelLayout(canvas: HTMLElement): void {
  const panel = canvas.querySelector(`.${BLOCK}__panel`);
  const img = canvas.querySelector<HTMLImageElement>(`.${BLOCK}__logo img`);
  if (!panel || !img?.complete) throw new Error('page not rendered yet');
  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);
  if (rect(panel).left < 15.5 || rect(panel).right > doc.clientWidth - 15.5) throw new Error('the panel has no 16px gutter');
  const inside = Array.from(canvas.querySelectorAll(`.${BLOCK}__logo img, .${BLOCK}__panel gbt-input input, .${BLOCK}__panel button, .${BLOCK}__panel a, .${BLOCK}__panel .gbt-alert, .${BLOCK}__panel gbt-skeleton`));
  for (const part of inside) {
    if (rect(part).left < rect(panel).left - 0.5 || rect(part).right > rect(panel).right + 0.5) throw new Error(`content spills out of the panel: ${part.tagName} ${part.className}`);
  }
  for (const tap of Array.from(canvas.querySelectorAll(`.gbt-auth-footer__link, .${BLOCK}__links .gbt-button, .${BLOCK}__submit button`))) {
    if (rect(tap).height < 43.5) throw new Error(`tap target under 44px: ${tap.className} ${rect(tap).height}px`);
  }
  if (doc.clientWidth <= 480) {
    for (const input of Array.from(canvas.querySelectorAll(`.${BLOCK}__panel gbt-input input`))) {
      if (rect(input).height < 43.5) throw new Error(`field under 44px: ${rect(input).height}px`);
    }
  }
  const button = canvas.querySelector(`.${BLOCK}__submit button`);
  const field = canvas.querySelector(`.${BLOCK}__panel .gbt-input`);
  if (button && field && Math.abs(rect(button).width - rect(field).width) > 1) throw new Error('the button should span the form');
}

export function expectPanelLayout() {
  return ({ canvasElement }: { canvasElement: HTMLElement }) => waitFor(() => assertPanelLayout(canvasElement), { timeout: 3000 });
}
