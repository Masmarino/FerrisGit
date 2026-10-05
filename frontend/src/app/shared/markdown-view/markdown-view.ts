import { afterRenderEffect, Component, computed, ElementRef, inject, input, output, viewChild } from '@angular/core';
import { DomSanitizer, SafeHtml } from '@angular/platform-browser';
import { Router } from '@angular/router';
import { marked, Token } from 'marked';
import DOMPurify, { Config } from 'dompurify';

// Markdown comes from any collaborator, so it's sanitised harder than DOMPurify's default: no restyling the page
// (<style>, style="…"), no fake UI (forms, buttons, dialogs), no colliding ids. A dedicated instance keeps these hooks
// off the global DOMPurify that code-view also uses.
const purify = DOMPurify(window);

// Only GFM task-list checkboxes survive; any other <input> (password, text, hidden…) goes.
purify.addHook('uponSanitizeElement', (node, data) => {
  if (data.tagName === 'input' && (node as Element).getAttribute('type')?.toLowerCase() !== 'checkbox') {
    node.parentNode?.removeChild(node);
  }
});

purify.addHook('afterSanitizeAttributes', (node) => {
  // A task-list checkbox is a mark, not a control.
  if (node.nodeName === 'INPUT') {
    node.setAttribute('disabled', '');
  }
  // Remote images are fine (badges, screenshots) but mustn't leak the page URL to their host or load before they're
  // scrolled into view.
  if (node.nodeName === 'IMG') {
    node.setAttribute('referrerpolicy', 'no-referrer');
    node.setAttribute('loading', 'lazy');
  }
});

// Authors can't borrow the app's global CSS classes (sr-only, skip-link, form errors, tooltips…) to spoof its UI. Only
// the `language-*` classes of fenced code blocks survive, for syntax highlighting.
purify.addHook('uponSanitizeAttribute', (_node, data) => {
  if (data.attrName === 'class') {
    const kept = data.attrValue.split(/\s+/).filter((c) => /^language-[\w+#.-]+$/.test(c));
    if (kept.length) {
      data.attrValue = kept.join(' ');
    } else {
      data.keepAttr = false;
    }
  }
});

const SANITIZE_CONFIG: Config = {
  FORBID_TAGS: ['style', 'form', 'button', 'textarea', 'select', 'option', 'optgroup', 'fieldset', 'dialog'],
  // `for` and aria IDREFs could point at the page's own controls (a <label for> toggling a real switch), and tabindex
  // could hijack the tab order.
  FORBID_ATTR: [
    'style',
    'popover',
    'popovertarget',
    'popovertargetaction',
    'for',
    'tabindex',
    'aria-owns',
    'aria-controls',
    'aria-labelledby',
    'aria-describedby',
  ],
  // `id` and `name` become "user-content-…" so markdown can't clobber the page's ids.
  SANITIZE_NAMED_PROPS: true,
};

/** One heading of the rendered content, for a "Sur cette page" panel. */
export interface MarkdownOutlineEntry {
  /** The level as rendered, after `headingOffset`: 2, 3 or 4. */
  level: number;
  text: string;
  id: string;
}

const ID_PREFIX = 'user-content-';

/** GitHub-like slug, accents kept. `section` when nothing is left, e.g. a title made only of emoji. */
export function headingSlug(text: string): string {
  const slug = text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .trim()
    .replace(/[\s-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return slug || 'section';
}

/**
 * Gives every rendered h1–h4 a stable `user-content-<slug>` id and returns the h2–h4 outline. Runs after sanitising so
 * ids are never the author's: an author id that would duplicate a heading id is removed. Headings in a quote or
 * `<details>` get an id but no outline entry.
 */
function applyHeadingIds(root: HTMLElement): MarkdownOutlineEntry[] {
  const headings = Array.from(root.querySelectorAll<HTMLHeadingElement>('h1, h2, h3, h4'));
  const used = new Set<string>();
  const outline: MarkdownOutlineEntry[] = [];
  for (const heading of headings) {
    const text = (heading.textContent ?? '').replace(/\s+/g, ' ').trim();
    const base = ID_PREFIX + headingSlug(text);
    let id = base;
    for (let n = 2; used.has(id); n++) {
      id = `${base}-${n}`;
    }
    used.add(id);
    heading.id = id;
    const level = Number(heading.tagName[1]);
    if (level >= 2 && !heading.parentElement?.closest('blockquote, details')) {
      outline.push({ level, text, id });
    }
  }
  for (const node of Array.from(root.querySelectorAll('[id]'))) {
    if (used.has(node.id) && !headings.includes(node as HTMLHeadingElement)) {
      node.removeAttribute('id');
    }
  }
  return outline;
}

export function decodeFragment(rawFragment: string): string {
  try {
    return decodeURIComponent(rawFragment);
  } catch {
    return rawFragment;
  }
}

/** The element a `#fragment` points at: the plain `id`, else the `user-content-` id or `<a name>` the sanitiser made. */
export function findAnchorTarget(root: HTMLElement, rawFragment: string): HTMLElement | null {
  const fragment = decodeFragment(rawFragment);
  if (!fragment) {
    return null;
  }
  const candidates = [fragment, ID_PREFIX + fragment];
  for (const wanted of candidates) {
    for (const node of Array.from(root.querySelectorAll<HTMLElement>('[id], a[name]'))) {
      if (node.id === wanted || (node.tagName === 'A' && node.getAttribute('name') === wanted)) {
        return node;
      }
    }
  }
  return null;
}

/**
 * A long code block or a wide table scrolls sideways, which a keyboard can only do once it takes focus (WCAG 2.1.1):
 * each one becomes a named stop in the tab order. A table keeps its own role and gets only the stop and the name.
 */
export function makeScrollableBlocksFocusable(container: HTMLElement): void {
  for (const pre of Array.from(container.querySelectorAll('pre'))) {
    pre.setAttribute('tabindex', '0');
    pre.setAttribute('role', 'region');
    pre.setAttribute('aria-label', 'Bloc de code');
  }
  for (const table of Array.from(container.querySelectorAll('table'))) {
    table.setAttribute('tabindex', '0');
    table.setAttribute('aria-label', 'Tableau');
  }
}

@Component({
  selector: 'fg-markdown-view',
  standalone: true,
  template: `<div class="markdown-view" #container [innerHTML]="renderedHtml()" (click)="onClick($event)"></div>`,
})
export class MarkdownView {
  content = input.required<string>();
  /** Levels added to every markdown heading (up to h6), so a `# Titre` under the page's own headings keeps the outline. */
  headingOffset = input(0);
  /**
   * Links under this prefix (`/docs` → `/docs/ci-cd/reference-yaml#variables`) go through the router instead of
   * reloading the app. Off by default: a README link is left to the browser.
   */
  routedLinkPrefix = input<string | null>(null);
  /** The h2–h4 headings, emitted after each render once their ids are in the DOM. */
  outline = output<MarkdownOutlineEntry[]>();

  private sanitizer = inject(DomSanitizer);
  private router = inject(Router, { optional: true });
  private container = viewChild.required<ElementRef<HTMLElement>>('container');

  constructor() {
    afterRenderEffect({
      write: () => {
        this.renderedHtml();
        const container = this.container().nativeElement;
        makeScrollableBlocksFocusable(container);
        this.outline.emit(applyHeadingIds(container));
      },
    });
  }

  /**
   * In-content `#anchor` links would resolve against `<base href="/">` and load the home page, so a plain left click
   * scrolls to the target inside this view and focuses it. Modified and middle clicks are left to the browser.
   */
  protected onClick(event: MouseEvent): void {
    if (event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) {
      return;
    }
    const container = this.container().nativeElement;
    const routed = this.routedHref(event.target);
    if (routed !== null) {
      event.preventDefault();
      void this.router!.navigateByUrl(routed);
      return;
    }
    const link = event.target instanceof Element ? event.target.closest('a[href^="#"]') : null;
    if (!link || !container.contains(link)) {
      return;
    }
    event.preventDefault();
    const target = findAnchorTarget(container, link.getAttribute('href')!.slice(1));
    if (!target) {
      return;
    }
    const reduceMotion = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    target.scrollIntoView?.({ behavior: reduceMotion ? 'auto' : 'smooth', block: 'start' });
    // Headings aren't focusable; -1 lets the next Tab carry on from the section just reached.
    if (target.tabIndex < 0) {
      target.setAttribute('tabindex', '-1');
    }
    target.focus({ preventScroll: true });
  }

  /** The `href` of a clicked in-content link under `routedLinkPrefix`, or `null` when the browser should follow it. */
  private routedHref(target: EventTarget | null): string | null {
    const prefix = this.routedLinkPrefix();
    const link = prefix && this.router && target instanceof Element ? target.closest('a[href]') : null;
    if (!link || !this.container().nativeElement.contains(link) || link.hasAttribute('target')) {
      return null;
    }
    const href = link.getAttribute('href')!;
    const rest = href.startsWith(prefix!) ? href.slice(prefix!.length) : null;
    return rest !== null && (rest === '' || /^[/#?]/.test(rest)) ? href : null;
  }

  // marked.parse is synchronous unless an async extension is registered (none is), so the cast is safe. A `walkTokens`
  // passed per call only applies to that call.
  protected renderedHtml = computed<SafeHtml>(() => {
    const offset = this.headingOffset();
    const walkTokens =
      offset > 0
        ? (token: Token) => {
            if (token.type === 'heading') {
              token.depth = Math.min(6, token.depth + offset);
            }
          }
        : null;
    const rawHtml = marked.parse(this.content(), { async: false, walkTokens }) as string;
    const cleanHtml = purify.sanitize(rawHtml, SANITIZE_CONFIG);
    return this.sanitizer.bypassSecurityTrustHtml(cleanHtml);
  });
}
