import { afterRenderEffect, Component, computed, ElementRef, inject, input, output, viewChild } from '@angular/core';
import { DomSanitizer, SafeHtml } from '@angular/platform-browser';
import { marked, Token } from 'marked';
import DOMPurify, { Config } from 'dompurify';

// Markdown comes from any collaborator, so it is sanitised harder than DOMPurify's default: no page
// restyling (<style>, style="…"), no fake UI (forms, buttons, dialogs) and no colliding ids. A
// dedicated instance keeps these hooks off the global DOMPurify that `shared/code-view` also uses.
const purify = DOMPurify(window);

// Only GFM task-list checkboxes may stay. Any other <input> (password, text, hidden…) is removed.
purify.addHook('uponSanitizeElement', (node, data) => {
  if (data.tagName === 'input' && (node as Element).getAttribute('type')?.toLowerCase() !== 'checkbox') {
    node.parentNode?.removeChild(node);
  }
});

purify.addHook('afterSanitizeAttributes', (node) => {
  // A task-list checkbox is a read-only mark here, not a control.
  if (node.nodeName === 'INPUT') {
    node.setAttribute('disabled', '');
  }
  // Remote images are allowed (badges, screenshots), but they should not leak the page URL to their host
  // or load before they scroll into view.
  if (node.nodeName === 'IMG') {
    node.setAttribute('referrerpolicy', 'no-referrer');
    node.setAttribute('loading', 'lazy');
  }
});

// Authors may not borrow the app's global CSS classes (sr-only, skip-link, form errors, tooltips…) to
// spoof its UI: only the `language-*` classes of fenced code blocks (syntax highlighting) survive.
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
  // `for` / aria IDREFs would point at the page's own controls (a <label for> toggling a real switch),
  // and tabindex could take over the tab order.
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
  // `id="…"` and `name="…"` become "user-content-…", so markdown cannot clobber or collide with page ids.
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

/** GitHub-like slug (accents kept). `section` when nothing is left, for example a title made only of emoji. */
function headingSlug(text: string): string {
  const slug = text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .trim()
    .replace(/[\s-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return slug || 'section';
}

/**
 * Gives every rendered h1–h4 a stable `user-content-<slug>` id and returns the h2–h4 outline. Runs
 * after sanitisation so ids are never author-controlled: an author id that would duplicate a heading
 * id is removed. Headings in a quote or `<details>` get an id but no outline entry.
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

/** The element a `#fragment` link points at: the `id` itself, else the `user-content-` id or `<a name>` the sanitiser produced. */
function findAnchorTarget(root: HTMLElement, rawFragment: string): HTMLElement | null {
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

@Component({
  selector: 'fg-markdown-view',
  standalone: true,
  template: `<div class="markdown-view" #container [innerHTML]="renderedHtml()" (click)="onClick($event)"></div>`,
})
export class MarkdownView {
  content = input.required<string>();
  /**
   * Levels added to every markdown heading (capped at h6), so a `# Titre` nested under the page's own
   * headings keeps the document outline.
   */
  headingOffset = input(0);
  /** The h2–h4 headings, emitted after each render (their ids are on the DOM by then). */
  outline = output<MarkdownOutlineEntry[]>();

  private sanitizer = inject(DomSanitizer);
  private container = viewChild.required<ElementRef<HTMLElement>>('container');

  constructor() {
    afterRenderEffect({
      write: () => {
        this.renderedHtml();
        this.outline.emit(applyHeadingIds(this.container().nativeElement));
      },
    });
  }

  /**
   * In-content `#anchor` links would resolve against the app's `<base href="/">` and load the home
   * page. Instead, a plain left click scrolls to the target inside this view and moves focus there.
   * Modified and middle clicks are left to the browser.
   */
  protected onClick(event: MouseEvent): void {
    if (event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) {
      return;
    }
    const container = this.container().nativeElement;
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
    // Headings are not focusable: -1 lets the next Tab continue from the section just reached.
    if (target.tabIndex < 0) {
      target.setAttribute('tabindex', '-1');
    }
    target.focus({ preventScroll: true });
  }

  // `marked.parse` is synchronous unless an async extension is used (none is), so the cast is safe.
  // `walkTokens` passed per call applies to this call only.
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
