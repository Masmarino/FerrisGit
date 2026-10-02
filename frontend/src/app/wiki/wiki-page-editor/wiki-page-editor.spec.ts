import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { WikiPageEditor } from './wiki-page-editor';
import { WikiService } from '../wiki.service';
import { GbtToastService } from '@masmarino/gabarit';
import { RepositoryContextService } from '../../repositories/repository-context.service';

describe('WikiPageEditor', () => {
  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const buttonByText = (root: Element, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);

  function setup(slug: string | null, existingPages: { slug: string; title: string }[] = []) {
    const wikiStub = {
      list: vi.fn(() => of({ headSha: 'wiki-head', pages: existingPages })),
      detail: vi.fn(() => of({ content: '# existing', headSha: 'page-head', title: 'Existing' })),
      save: vi.fn(() => of({ content: 'saved', headSha: 'new-head', title: 'x' })),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [provideRouter([]), { provide: WikiService, useValue: wikiStub }, { provide: GbtToastService, useValue: toastStub }],
    });
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'r1', path: ['alice', 'hello'], role: 'contributor', ancestors: [], groupId: null });
    const fixture = TestBed.createComponent(WikiPageEditor);
    fixture.componentRef.setInput('repositoryId', 'r1');
    fixture.componentRef.setInput('slug', slug);
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance, wikiStub, toastStub, router: TestBed.inject(Router), el: fixture.nativeElement as HTMLElement };
  }

  it('create mode fetches the wiki-level headSha as baseSha, not a page detail', () => {
    const { component, wikiStub } = setup(null);
    expect(wikiStub.list).toHaveBeenCalledWith('r1');
    // One request: the shell's page list also gives the editor its baseSha and the taken slugs.
    expect(wikiStub.list).toHaveBeenCalledTimes(1);
    expect(wikiStub.detail).not.toHaveBeenCalled();
    expect(component.baseSha()).toBe('wiki-head');
  });

  it('edit mode fetches the existing page content and its own headSha', () => {
    const { component, wikiStub } = setup('Existing');
    expect(wikiStub.detail).toHaveBeenCalledWith('r1', 'Existing');
    expect(component.content()).toBe('# existing');
    expect(component.baseSha()).toBe('page-head');
  });

  it('on a 409 conflict, keeps the typed content and shows a reload message', () => {
    const { component, wikiStub, toastStub } = setup('Existing');
    wikiStub.save.mockReturnValue(throwError(() => ({ status: 409 })));
    component.content.set('my in-progress edit');
    component.save();
    expect(component.content()).toBe('my in-progress edit');
    expect(toastStub.show).toHaveBeenCalledWith(expect.stringContaining('modifié'), 'error');
  });

  it('on a 409 conflict, also keeps the explanation on the page, above the editor', () => {
    const { fixture, component, wikiStub, el } = setup('Existing');
    wikiStub.save.mockReturnValue(throwError(() => ({ status: 409 })));
    component.save();
    fixture.detectChanges();
    const alert = el.querySelector('gbt-alert.wiki-page-editor__conflict')!;
    expect(alert).toBeTruthy();
    expect(alert.querySelector('[data-variant="warning"]')).toBeTruthy();
    expect(text(alert)).toContain('Quelqu\'un d\'autre a modifié cette page');
  });

  it('navigates to the page view on a successful save', () => {
    const { component, wikiStub, router } = setup(null);
    // The test router has no routes, so a real navigation would reject with NG04002: `navigate` is stubbed.
    const navigateSpy = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    component.newSlug.set('New-Page');
    component.save();
    expect(wikiStub.save).toHaveBeenCalled();
    expect(navigateSpy).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki', 'New-Page']);
  });

  it('create mode blocks the save and shows a collision message when the typed slug already exists', () => {
    const { component, wikiStub, toastStub } = setup(null, [{ slug: 'Home', title: 'Home' }]);
    component.newSlug.set('Home');
    component.save();
    expect(wikiStub.save).not.toHaveBeenCalled();
    expect(toastStub.show).toHaveBeenCalledWith(expect.stringContaining('Home'), 'error');
    expect(toastStub.show).toHaveBeenCalledWith(expect.stringContaining('existe déjà'), 'error');
  });

  it('create mode blocks the save when the slug breaks the naming rules', () => {
    const { component, wikiStub, toastStub } = setup(null);
    for (const slug of ['', '-leading-dash', 'avec espace', 'a/b', 'x'.repeat(101)]) {
      component.newSlug.set(slug);
      component.save();
    }
    expect(wikiStub.save).not.toHaveBeenCalled();
    expect(toastStub.show).toHaveBeenCalledWith('Le nom de la page doit contenir uniquement des lettres, chiffres, "-" et "_"', 'error');
  });

  it('create mode still saves normally when the typed slug is genuinely new', () => {
    const { component, wikiStub, toastStub, router } = setup(null, [{ slug: 'Home', title: 'Home' }]);
    // The test router has no routes, so a real navigation would reject with NG04002: `navigate` is stubbed.
    vi.spyOn(router, 'navigate').mockResolvedValue(true);
    component.newSlug.set('Getting-Started');
    component.save();
    expect(wikiStub.save).toHaveBeenCalledWith('r1', 'Getting-Started', { content: '', baseSha: 'wiki-head' });
    expect(toastStub.show).toHaveBeenCalledWith('Page wiki créée.');
  });

  it('sends the optional commit message, trimmed, when one is typed', () => {
    const { fixture, wikiStub, router, el } = setup('Existing');
    vi.spyOn(router, 'navigate').mockResolvedValue(true);
    const field = el.querySelector<HTMLElement>('.wiki-page-editor__message')!;
    expect(text(field.querySelector('label'))).toBe('Message de commit');
    expect(field.querySelector('input')!.getAttribute('placeholder')).toBe('Update Existing');
    const input = field.querySelector('input')!;
    input.value = '  Corriger les liens  ';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    buttonByText(el, 'Enregistrer')!.click();
    expect(wikiStub.save).toHaveBeenCalledWith('r1', 'Existing', { content: '# existing', baseSha: 'page-head', message: 'Corriger les liens' });
  });

  it('is built on the wiki shell (wide layout, the edited page current in the nav)', () => {
    const { el } = setup('Existing', [{ slug: 'Existing', title: 'Existing' }]);
    expect(el.querySelector('fg-wiki-layout gbt-page-layout[data-width="wide"]')).toBeTruthy();
    expect(el.querySelector('.container-wide')).toBeNull();
    expect(text(el.querySelector('.wiki-nav__list a[aria-current="page"]'))).toBe('Existing');
  });

  it('heads the form "Nouvelle page" or "Modifier <title>"', () => {
    const create = setup(null);
    expect(text(create.el.querySelector('h1'))).toBe('Nouvelle page');
    TestBed.resetTestingModule();
    const edit = setup('Existing');
    expect(text(edit.el.querySelector('h1'))).toBe('Modifier Existing');
  });

  it('lays the source and the preview side by side in a card that stacks them when narrow', () => {
    const { fixture, component, el } = setup('Existing');
    const card = el.querySelector('gbt-list-card')!;
    const panes = card.querySelector('.wiki-page-editor__panes')!;
    expect(Array.from(panes.children).map((pane) => pane.className)).toEqual(['wiki-page-editor__pane wiki-page-editor__source', 'wiki-page-editor__pane wiki-page-editor__preview']);
    const textarea = panes.querySelector<HTMLTextAreaElement>('.wiki-page-editor__source textarea')!;
    expect(textarea.labels?.[0] && text(textarea.labels[0])).toBe('Markdown');
    const preview = panes.querySelector('.wiki-page-editor__preview')!;
    expect(preview.getAttribute('role')).toBe('region');
    expect(text(el.querySelector('#' + preview.getAttribute('aria-labelledby')))).toBe('Aperçu');

    textarea.value = '## Nouveau titre';
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(component.content()).toBe('## Nouveau titre');
    expect(text(preview.querySelector('fg-markdown-view h2, fg-markdown-view h3'))).toBe('Nouveau titre');
  });

  it('shows a placeholder in the preview while the source is empty', () => {
    const { el } = setup(null);
    expect(text(el.querySelector('.wiki-page-editor__preview-empty'))).toBe('L’aperçu s’affichera ici.');
  });

  it('offers "Enregistrer" as the only primary button, next to "Annuler"', () => {
    const { el } = setup('Existing');
    const primaries = el.querySelectorAll('.gbt-button--primary');
    expect(primaries.length).toBe(1);
    expect(text(primaries[0])).toBe('Enregistrer');
    expect(buttonByText(el.querySelector('.wiki-page-editor__actions')!, 'Annuler')!.classList).toContain('gbt-button--secondary');
  });

  it('names the create button after what it does', () => {
    const { el } = setup(null);
    expect(text(el.querySelector('.gbt-button--primary'))).toBe('Créer la page');
  });

  it('"Annuler" goes back to the page when editing, to the wiki when creating', () => {
    const edit = setup('Existing');
    const navigate = vi.spyOn(edit.router, 'navigate').mockResolvedValue(true);
    buttonByText(edit.el, 'Annuler')!.click();
    expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki', 'Existing']);
    TestBed.resetTestingModule();

    const create = setup(null);
    const navigateCreate = vi.spyOn(create.router, 'navigate').mockResolvedValue(true);
    buttonByText(create.el, 'Annuler')!.click();
    expect(navigateCreate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki']);
  });

  it('does not offer "Nouvelle page" in the nav of the creation form itself', () => {
    const create = setup(null);
    expect(create.el.querySelector('.wiki-nav__new')).toBeNull();
    TestBed.resetTestingModule();
    const edit = setup('Existing');
    expect(edit.el.querySelector('.wiki-nav__new')).toBeTruthy();
  });

  it('asks for the page name only when creating, with its rules as a hint', () => {
    const create = setup(null);
    const field = create.el.querySelector('.wiki-page-editor__slug')!;
    expect(text(field.querySelector('label'))).toBe('Nom de la page');
    const input = field.querySelector('input')!;
    expect(text(create.el.querySelector(`#${input.getAttribute('aria-describedby')}`))).toBe('Lettres, chiffres, « - » et « _ » : il devient l’adresse de la page.');
    TestBed.resetTestingModule();
    const edit = setup('Existing');
    expect(edit.el.querySelector('.wiki-page-editor__slug')).toBeNull();
  });
});
