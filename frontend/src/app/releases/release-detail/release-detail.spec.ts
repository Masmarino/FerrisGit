import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { NEVER, Observable, of, Subject, throwError } from 'rxjs';
import { ReleaseDetail } from './release-detail';
import { ReleaseAsset, ReleaseDetail as ReleaseDetailResponse, ReleasesService } from '../releases.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

type Role = 'owner' | 'reader' | 'contributor' | 'maintainer';

describe('ReleaseDetail', () => {
  afterEach(() => vi.restoreAllMocks());

  const baseRelease: ReleaseDetailResponse = {
    id: '1',
    tagName: 'v1.0.0',
    title: 'First release',
    notes: 'Some **notes**',
    draft: false,
    prerelease: false,
    targetCommitSha: 'abc123',
    authorId: 'a',
    author: null,
    createdAt: '2026-01-01T00:00:00Z',
    publishedAt: '2026-01-01T00:00:00Z',
    assets: [],
  };

  function setup(role: Role, release: object = baseRelease, detail?: () => Observable<unknown>) {
    const releasesStub = {
      detail: vi.fn(detail ?? (() => of(release))),
      update: vi.fn(),
      delete: vi.fn(),
      uploadAsset: vi.fn(),
      deleteAsset: vi.fn(),
      downloadAsset: vi.fn(() => of(new Blob(['content']))),
    };
    const pageTitleStub = { set: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: ReleasesService, useValue: releasesStub },
        { provide: PageTitleService, useValue: pageTitleStub },
        { provide: LOCALE_ID, useValue: 'fr' },
      ],
    });
    const fixture = TestBed.createComponent(ReleaseDetail);
    fixture.componentRef.setInput('repositoryId', 'r1');
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    fixture.componentRef.setInput('tagName', 'v1.0.0');
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'r1', path: ['alice', 'hello'], role, ancestors: [], groupId: null });
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, component: fixture.componentInstance, releasesStub, pageTitleStub, el };
  }

  const text = (el: Element | null | undefined) => {
    if (!el) return '';
    const clone = el.cloneNode(true) as Element;
    clone.querySelectorAll('gbt-avatar').forEach((avatar) => avatar.remove());
    return (clone.textContent ?? '').replace(/\s+/g, ' ').trim();
  };
  const buttonByText = (root: Element, label: string) =>
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);
  const headerActions = (el: HTMLElement) => el.querySelector('.gbt-page-header__actions')!;
  const dialog = (el: HTMLElement) => el.querySelector<HTMLElement>('[role="dialog"]');
  const panel = (el: HTMLElement, heading: string) =>
    Array.from(el.querySelectorAll<HTMLElement>('.gbt-page-layout__aside gbt-panel')).find((p) => text(p.querySelector('.gbt-panel__heading')) === heading)!;

  const facts = (root: Element) => Array.from(root.querySelectorAll('dt')).map((dt) => [text(dt), text(dt.nextElementSibling)]);

  function type(input: HTMLInputElement | HTMLTextAreaElement, value: string, fixture: { detectChanges(): void }) {
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  const asset: ReleaseAsset = { id: 'a1', filename: 'archive.tar.gz', contentType: 'application/gzip', sizeBytes: 10, uploadedBy: 'u', uploader: null, createdAt: '2026-01-01T00:00:00Z' };

  describe('page frame and header', () => {
    it('renders the release title and target commit', () => {
      const { fixture } = setup('reader');
      expect(fixture.nativeElement.textContent).toContain('First release');
      expect(fixture.nativeElement.textContent).toContain('abc123');
    });

    it('lays the page out as a wide page layout with the aside panels', () => {
      const { el } = setup('reader');
      const layout = el.querySelector('gbt-page-layout')!;
      expect(layout.getAttribute('data-width')).toBe('wide');
      expect(layout.getAttribute('data-aside-width')).toBe('md');
      expect(Array.from(el.querySelectorAll('.gbt-page-layout__aside .gbt-panel__heading')).map(text)).toEqual(['Tag', 'Commit cible', 'Auteur', 'Publication', 'Téléchargements']);
    });

    it('titles the page with the release title (the h1), the tag chip and the status beside it, and sets the tab title', () => {
      const { el, pageTitleStub } = setup('reader');

      expect(text(el.querySelector('gbt-page-header h1'))).toBe('First release');
      const badges = el.querySelector('.gbt-page-header__badges')!;
      // Direct child only: fg-status-badge, the next sibling, renders its own gbt-badge nested inside it.
      expect(text(badges.querySelector(':scope > gbt-badge'))).toBe('v1.0.0');
      expect(text(badges.querySelector('fg-status-badge'))).toBe('Publiée');
      expect(pageTitleStub.set).toHaveBeenCalledWith('First release');
    });

    it('says when and by whom the release was published', () => {
      const { el } = setup('reader', { ...baseRelease, author: { id: 'a', username: 'alice' } });
      const meta = el.querySelector('.release-detail__meta')!;
      expect(text(meta)).toBe(`publiée ${relativeTime(baseRelease.publishedAt!)} par alice`);
      expect(meta.querySelector('time')!.getAttribute('title')).toBe(absoluteDateTime(baseRelease.publishedAt!));
    });

    it('dates a draft by its creation, and names a deleted author "Utilisateur supprimé"', () => {
      const createdAt = '2026-02-02T00:00:00Z';
      const { el } = setup('maintainer', { ...baseRelease, draft: true, publishedAt: null, createdAt, author: null });
      expect(text(el.querySelector('.release-detail__meta'))).toBe(`créée ${relativeTime(createdAt)} par Utilisateur supprimé`);
      expect(text(el.querySelector('.gbt-page-header__badges fg-status-badge'))).toBe('Brouillon');
    });

    it('reads a published pre-release as "Pré-version"', () => {
      const { el } = setup('reader', { ...baseRelease, prerelease: true });
      expect(text(el.querySelector('.gbt-page-header__badges fg-status-badge'))).toBe('Pré-version');
    });
  });

  describe('actions', () => {
    it('hides edit/publish/delete actions from a non-Maintainer', () => {
      const { component, el } = setup('contributor');
      expect(component.canManage()).toBe(false);
      expect(el.querySelector('.gbt-page-header__actions gbt-button')).toBeNull();
    });

    it('does not show a publish button for an already-published release', () => {
      const { fixture } = setup('maintainer', { ...baseRelease, draft: false });
      expect(fixture.nativeElement.textContent).not.toContain('Publier');
    });

    it('offers "Modifier" (secondary) and "Supprimer" (danger) on a published release, and no primary button', () => {
      const { el } = setup('maintainer');
      const actions = headerActions(el);
      expect(buttonByText(actions, 'Modifier')!.classList).toContain('gbt-button--secondary');
      expect(buttonByText(actions, 'Supprimer')!.classList).toContain('gbt-button--danger');
      expect(el.querySelector('.gbt-button--primary')).toBeNull();
    });

    it('shows a publish button for a draft release, and publishing calls update with draft: false', () => {
      const { fixture, component, releasesStub } = setup('maintainer', { ...baseRelease, draft: true });
      expect(fixture.nativeElement.textContent).toContain('Publier');

      releasesStub.update.mockReturnValue(of({ ...baseRelease, draft: false }));
      component.publish();

      expect(releasesStub.update).toHaveBeenCalledWith('r1', 'v1.0.0', { draft: false });
    });

    it('makes "Publier" the one primary button of a draft, and publishes from it', () => {
      const { fixture, el, releasesStub } = setup('maintainer', { ...baseRelease, draft: true, publishedAt: null });
      const primaries = Array.from(el.querySelectorAll('.gbt-button--primary'));
      expect(primaries.map(text)).toEqual(['Publier']);

      releasesStub.update.mockReturnValue(of({ ...baseRelease, draft: false }));
      (primaries[0] as HTMLButtonElement).click();
      fixture.detectChanges();

      expect(releasesStub.update).toHaveBeenCalledWith('r1', 'v1.0.0', { draft: false });
      expect(text(el.querySelector('.gbt-page-header__badges fg-status-badge'))).toBe('Publiée');
      expect(buttonByText(headerActions(el), 'Publier')).toBeUndefined();
    });
  });

  describe('deleting the release', () => {
    it('deletes the release without touching git tags, only via ReleasesService.delete', () => {
      const { fixture, el, releasesStub } = setup('maintainer');
      releasesStub.delete.mockReturnValue(of(undefined));
      const confirmSpy = vi.spyOn(window, 'confirm');
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      buttonByText(headerActions(el), 'Supprimer')!.click();
      fixture.detectChanges();
      const modal = dialog(el)!;
      expect(modal.getAttribute('aria-label')).toBe('Supprimer la release');
      expect(text(modal)).toContain('Le tag git « v1.0.0 » est conservé');
      type(modal.querySelector('input')!, 'v1.0.0', fixture);
      buttonByText(modal, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(releasesStub.delete).toHaveBeenCalledWith('r1', 'v1.0.0');
      expect(releasesStub.delete).toHaveBeenCalledTimes(1);
      expect(confirmSpy).not.toHaveBeenCalled();
      expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'releases']);
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Release supprimée.' });
    });

    it('does not delete when the confirmation is declined', () => {
      const { fixture, el, releasesStub } = setup('maintainer');

      buttonByText(headerActions(el), 'Supprimer')!.click();
      fixture.detectChanges();
      buttonByText(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();

      expect(dialog(el)).toBeNull();
      expect(releasesStub.delete).not.toHaveBeenCalled();
    });

    it('keeps the confirm button disabled until the tag is typed', () => {
      const { fixture, el } = setup('maintainer');
      buttonByText(headerActions(el), 'Supprimer')!.click();
      fixture.detectChanges();

      expect(buttonByText(dialog(el)!, 'Supprimer')!.disabled).toBe(true);
    });

    it('stays on the page with an error toast when the deletion fails', () => {
      const { fixture, el, component, releasesStub } = setup('maintainer');
      releasesStub.delete.mockReturnValue(throwError(() => new Error('500')));
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate');

      component.deleteRelease();
      fixture.detectChanges();
      (component as unknown as { confirmDeleteRelease(): void }).confirmDeleteRelease();
      fixture.detectChanges();

      expect(navigate).not.toHaveBeenCalled();
      expect(dialog(el)).toBeNull();
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de supprimer la release.', variant: 'error' });
    });
  });

  describe('notes', () => {
    it('renders release notes as markdown', () => {
      const { fixture } = setup('reader');
      expect(fixture.nativeElement.querySelector('strong')?.textContent).toBe('notes');
    });

    it('puts the notes in a "Notes de version" card, their headings under the card’s h2', () => {
      const { el } = setup('reader', { ...baseRelease, notes: '# Nouveautés\n\nTexte' });
      const card = el.querySelector('.release-detail__notes')!;
      expect(text(card.querySelector('h2'))).toBe('Notes de version');
      expect(text(card.querySelector('fg-markdown-view h3'))).toBe('Nouveautés');
      expect(card.querySelector('fg-markdown-view h1')).toBeNull();
    });

    it('draws its cards as outlined cards under their heading: the notes and the files, then the edit form in place of the notes', () => {
      const { el, fixture } = setup('maintainer');
      const inner = (card: Element) => card.querySelector(':scope > .gbt-card')!;
      expect(el.querySelectorAll('.release-detail__main gbt-card').length).toBe(2);
      for (const card of Array.from(el.querySelectorAll('.release-detail__main gbt-card'))) {
        expect(inner(card).getAttribute('data-variant')).toBe('outlined');
        expect(Array.from(card.children)).toEqual([card.querySelector(':scope > .gbt-card__header'), inner(card)]);
      }
      expect(inner(el.querySelector('.release-detail__assets')!).hasAttribute('data-flush')).toBe(true);

      buttonByText(headerActions(el), 'Modifier')!.click();
      fixture.detectChanges();
      const edit = el.querySelector('.release-detail__edit')!;
      expect(edit.tagName).toBe('GBT-CARD');
      expect(text(edit.querySelector('h2'))).toBe('Modifier la release');
      expect(el.querySelector('.release-detail__notes')).toBeNull();
    });

    it('says so when the release has no notes', () => {
      const { el } = setup('reader', { ...baseRelease, notes: '' });
      expect(text(el.querySelector('.release-detail__notes-empty'))).toBe('Aucune note de version');
    });
  });

  describe('editing', () => {
    /** Opens the edit form. Its `ngModel` fields register a microtask later, so wait before reading or typing. */
    async function startEditing(role: Role = 'maintainer') {
      const ctx = setup(role, { ...baseRelease, prerelease: true });
      buttonByText(headerActions(ctx.el), 'Modifier')!.click();
      ctx.fixture.detectChanges();
      await ctx.fixture.whenStable();
      ctx.fixture.detectChanges();
      return ctx;
    }

    it('swaps the notes card for an edit form prefilled with the release, and hides the header actions', async () => {
      const { el } = await startEditing();

      const form = el.querySelector('.release-detail__edit')!;
      expect(text(form.querySelector('h2'))).toBe('Modifier la release');
      expect(form.querySelector<HTMLInputElement>('.release-detail__edit-title input')!.value).toBe('First release');
      expect(form.querySelector<HTMLTextAreaElement>('.release-detail__edit-notes textarea')!.value).toBe('Some **notes**');
      expect(form.querySelector<HTMLInputElement>('input[role="switch"]')!.checked).toBe(true);
      expect(el.querySelector('.release-detail__notes')).toBeNull();
      expect(el.querySelector('.gbt-page-header__actions gbt-button')).toBeNull();
    });

    it('saves the title, notes and pre-release flag, then shows the updated release', async () => {
      const { fixture, el, releasesStub } = await startEditing();
      releasesStub.update.mockReturnValue(of({ ...baseRelease, title: 'Renamed', notes: 'New **text**' }));

      type(el.querySelector<HTMLInputElement>('.release-detail__edit-title input')!, 'Renamed', fixture);
      type(el.querySelector<HTMLTextAreaElement>('.release-detail__edit-notes textarea')!, 'New **text**', fixture);
      buttonByText(el.querySelector('.release-detail__edit')!, 'Enregistrer')!.click();
      fixture.detectChanges();

      expect(releasesStub.update).toHaveBeenCalledWith('r1', 'v1.0.0', { title: 'Renamed', notes: 'New **text**', prerelease: true });
      expect(el.querySelector('.release-detail__edit')).toBeNull();
      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Renamed');
    });

    it('refuses an empty title with an error toast, without calling the API', async () => {
      const { fixture, el, releasesStub } = await startEditing();
      type(el.querySelector<HTMLInputElement>('.release-detail__edit-title input')!, '   ', fixture);
      buttonByText(el.querySelector('.release-detail__edit')!, 'Enregistrer')!.click();

      expect(releasesStub.update).not.toHaveBeenCalled();
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Le titre est requis', variant: 'error' });
    });

    it('keeps the form open with an error toast when saving fails', async () => {
      const { fixture, el, releasesStub } = await startEditing();
      releasesStub.update.mockReturnValue(throwError(() => new Error('500')));
      buttonByText(el.querySelector('.release-detail__edit')!, 'Enregistrer')!.click();
      fixture.detectChanges();

      expect(el.querySelector('.release-detail__edit')).toBeTruthy();
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: "Impossible d'enregistrer les modifications", variant: 'error' });
    });

    it('cancels back to the notes card, with the header actions', async () => {
      const { fixture, el, releasesStub } = await startEditing();
      buttonByText(el.querySelector('.release-detail__edit')!, 'Annuler')!.click();
      fixture.detectChanges();

      expect(el.querySelector('.release-detail__edit')).toBeNull();
      expect(el.querySelector('.release-detail__notes')).toBeTruthy();
      expect(buttonByText(headerActions(el), 'Modifier')).toBeTruthy();
      expect(releasesStub.update).not.toHaveBeenCalled();
    });

    it('has one primary button while editing: "Enregistrer"', async () => {
      const { el } = await startEditing();
      expect(Array.from(el.querySelectorAll('.gbt-button--primary')).map(text)).toEqual(['Enregistrer']);
    });
  });

  describe('files', () => {
    const assets: ReleaseAsset[] = [
      { ...asset, id: 'a1', filename: 'ferrisgit-linux.tar.gz', sizeBytes: 13_004_800, uploader: { id: 'u', username: 'bob' }, createdAt: '2026-01-02T00:00:00Z' },
      { ...asset, id: 'a2', filename: 'checksums.txt', sizeBytes: 812, uploader: null },
    ];
    const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.release-detail__assets-list > li'));
    // The download specs replace these statics (jsdom has no object URLs). Put the originals back, or delete the
    // property when there was none: assigning `undefined` would leave an own property behind and keep
    // `'createObjectURL' in URL` true for every later spec.
    const originals = (['createObjectURL', 'revokeObjectURL'] as const).map((name) => [name, Object.getOwnPropertyDescriptor(URL, name)] as const);
    afterEach(() => {
      for (const [name, descriptor] of originals) {
        if (descriptor) {
          Object.defineProperty(URL, name, descriptor);
        } else {
          delete (URL as unknown as Record<string, unknown>)[name];
        }
      }
    });

    it('puts a decorative file tile before each file', () => {
      const { el } = setup('reader', { ...baseRelease, assets });

      for (const row of rows(el)) {
        const marker = row.querySelector('gbt-icon-marker.release-detail__asset-icon .gbt-icon-marker');
        expect(marker?.getAttribute('data-shape')).toBe('tile');
        expect(marker?.getAttribute('aria-hidden')).toBe('true');
      }
    });

    it('lists each file with its name, size, uploader and date', () => {
      const { el } = setup('reader', { ...baseRelease, assets });

      const [first, second] = rows(el);
      expect(text(first.querySelector('.release-detail__asset-name'))).toBe('ferrisgit-linux.tar.gz');
      expect(text(first.querySelector('.release-detail__asset-meta'))).toBe(`12,4 Mo · ajouté ${relativeTime('2026-01-02T00:00:00Z')} par bob`);
      expect(text(first.querySelector('gbt-user-chip'))).toBe('bob');
      expect(first.querySelector('time')!.getAttribute('title')).toBe(absoluteDateTime('2026-01-02T00:00:00Z'));
      expect(text(second.querySelector('.release-detail__asset-meta'))).toBe(`812 o · ajouté ${relativeTime('2026-01-01T00:00:00Z')} par Utilisateur supprimé`);
      expect(second.querySelector('gbt-user-chip')).toBeNull();
    });

    it('counts the files in the card header', () => {
      const { el } = setup('reader', { ...baseRelease, assets });
      const header = el.querySelector('.release-detail__assets .gbt-card__header')!;
      expect(text(header.querySelector('h2'))).toBe('Fichiers joints 2');
      expect(text(header.querySelector('.gbt-card__count'))).toBe('2');
    });

    it('says so when there is no file', () => {
      const { el } = setup('reader');
      expect(rows(el).length).toBe(0);
      expect(text(el.querySelector('.release-detail__assets-empty'))).toBe('Aucun fichier joint');
    });

    it('offers a download button per file to a reader, but no delete button and no upload', () => {
      const { el } = setup('reader', { ...baseRelease, assets });
      const row = rows(el)[0];
      expect(row.querySelector('button[aria-label="Télécharger ferrisgit-linux.tar.gz"]')).toBeTruthy();
      expect(row.querySelector('button[aria-label="Supprimer ferrisgit-linux.tar.gz"]')).toBeNull();
      expect(el.querySelector('gbt-file-upload')).toBeNull();
    });

    it('draws the file actions as quiet icon-only buttons, the delete one going red on hover', () => {
      const { el } = setup('maintainer', { ...baseRelease, assets });
      const row = rows(el)[0];
      const download = row.querySelector<HTMLButtonElement>('button[aria-label="Télécharger ferrisgit-linux.tar.gz"]')!;
      const remove = row.querySelector<HTMLButtonElement>('button[aria-label="Supprimer ferrisgit-linux.tar.gz"]')!;

      expect(download.classList).toContain('gbt-button--icon-only');
      expect(download.classList).toContain('gbt-button--ghost');
      expect(remove.classList).toContain('gbt-button--icon-only');
      expect(remove.classList).toContain('gbt-button--ghost-danger');
    });

    it('downloads an asset by fetching it as a blob (with auth) and triggering a save via an object URL', () => {
      const { component, releasesStub } = setup('reader');
      const blob = new Blob(['content']);
      releasesStub.downloadAsset.mockReturnValue(of(blob));
      const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
      const createObjectURLSpy = vi.fn(() => 'blob:mock-url');
      const revokeObjectURLSpy = vi.fn();
      (URL as unknown as { createObjectURL: typeof createObjectURLSpy }).createObjectURL = createObjectURLSpy;
      (URL as unknown as { revokeObjectURL: typeof revokeObjectURLSpy }).revokeObjectURL = revokeObjectURLSpy;

      component.download(asset);

      expect(releasesStub.downloadAsset).toHaveBeenCalledWith('r1', 'v1.0.0', 'a1');
      expect(createObjectURLSpy).toHaveBeenCalledWith(blob);
      expect(clickSpy).toHaveBeenCalledTimes(1);
      expect(revokeObjectURLSpy).toHaveBeenCalledWith('blob:mock-url');

      clickSpy.mockRestore();
    });

    it('downloads from the row’s download button', () => {
      const { el, releasesStub } = setup('reader', { ...baseRelease, assets });
      const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
      (URL as unknown as { createObjectURL: () => string }).createObjectURL = () => 'blob:mock-url';
      (URL as unknown as { revokeObjectURL: () => void }).revokeObjectURL = () => {};

      rows(el)[1].querySelector<HTMLButtonElement>('button[aria-label="Télécharger checksums.txt"]')!.click();

      expect(releasesStub.downloadAsset).toHaveBeenCalledWith('r1', 'v1.0.0', 'a2');
      clickSpy.mockRestore();
    });

    it('shows a download error toast and does not crash when the asset fetch fails', () => {
      const { fixture, component, releasesStub } = setup('reader');
      const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
      releasesStub.downloadAsset.mockReturnValue(throwError(() => new Error('network error')));

      expect(() => component.download(asset)).not.toThrow();
      fixture.detectChanges();

      expect(showSpy).toHaveBeenCalledWith('Échec du téléchargement', 'error');
    });

    it('deletes a file after a light confirmation (no native confirm, no typed name), then drops its row', () => {
      const { fixture, el, releasesStub } = setup('maintainer', { ...baseRelease, assets });
      releasesStub.deleteAsset.mockReturnValue(of(undefined));
      const confirmSpy = vi.spyOn(window, 'confirm');

      rows(el)[1].querySelector<HTMLButtonElement>('button[aria-label="Supprimer checksums.txt"]')!.click();
      fixture.detectChanges();
      const modal = dialog(el)!;
      expect(modal.closest('gbt-confirm-danger-modal')).not.toBeNull();
      expect(modal.getAttribute('aria-label')).toBe('Supprimer le fichier');
      expect(text(modal)).toContain('checksums.txt');
      expect(modal.querySelector('input')).toBeNull();
      buttonByText(modal, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(releasesStub.deleteAsset).toHaveBeenCalledExactlyOnceWith('r1', 'v1.0.0', 'a2');
      expect(confirmSpy).not.toHaveBeenCalled();
      expect(dialog(el)).toBeNull();
      expect(rows(el).map((row) => text(row.querySelector('.release-detail__asset-name')))).toEqual(['ferrisgit-linux.tar.gz']);
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Fichier supprimé.' });
    });

    it('keeps the confirmation open and busy while the file is deleted: a second click sends nothing', () => {
      const { fixture, el, releasesStub } = setup('maintainer', { ...baseRelease, assets });
      const pending = new Subject<void>();
      releasesStub.deleteAsset.mockReturnValue(pending);

      rows(el)[1].querySelector<HTMLButtonElement>('button[aria-label="Supprimer checksums.txt"]')!.click();
      fixture.detectChanges();
      buttonByText(dialog(el)!, 'Supprimer')!.click();
      fixture.detectChanges();

      const confirm = dialog(el)!.querySelector<HTMLButtonElement>('.gbt-button--danger')!;
      expect(confirm.getAttribute('aria-busy')).toBe('true');
      confirm.click();
      buttonByText(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();
      expect(releasesStub.deleteAsset).toHaveBeenCalledTimes(1);
      expect(dialog(el)).not.toBeNull();

      pending.next();
      pending.complete();
      fixture.detectChanges();
      expect(dialog(el)).toBeNull();
      expect(rows(el).length).toBe(1);
    });

    it('closes the confirmation with an error toast when the deletion fails, keeping the file', () => {
      const { fixture, el, releasesStub } = setup('maintainer', { ...baseRelease, assets });
      releasesStub.deleteAsset.mockReturnValue(throwError(() => ({ status: 500 })));

      rows(el)[0].querySelector<HTMLButtonElement>('button[aria-label="Supprimer ferrisgit-linux.tar.gz"]')!.click();
      fixture.detectChanges();
      buttonByText(dialog(el)!, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(releasesStub.deleteAsset).toHaveBeenCalledWith('r1', 'v1.0.0', 'a1');
      expect(dialog(el)).toBeNull();
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de supprimer le fichier.', variant: 'error' });
      expect(rows(el).length).toBe(2);
    });

    it('keeps the file when its deletion is cancelled', () => {
      const { fixture, el, releasesStub } = setup('maintainer', { ...baseRelease, assets });
      rows(el)[0].querySelector<HTMLButtonElement>('button[aria-label="Supprimer ferrisgit-linux.tar.gz"]')!.click();
      fixture.detectChanges();
      buttonByText(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();

      expect(releasesStub.deleteAsset).not.toHaveBeenCalled();
      expect(dialog(el)).toBeNull();
      expect(rows(el).length).toBe(2);
    });

    it('uploads a picked file for a maintainer and appends it to the list', () => {
      const { fixture, el, component, releasesStub } = setup('maintainer');
      expect(el.querySelector('.release-detail__upload gbt-file-upload')).toBeTruthy();
      releasesStub.uploadAsset.mockReturnValue(of({ ...asset, filename: 'new.zip' }));
      const file = new File(['x'], 'new.zip');

      component.onFilesSelected([file]);
      fixture.detectChanges();

      expect(releasesStub.uploadAsset).toHaveBeenCalledWith('r1', 'v1.0.0', file);
      expect(rows(el).map((row) => text(row.querySelector('.release-detail__asset-name')))).toEqual(['new.zip']);
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Fichier ajouté.' });
    });

    it('says an upload is in progress', () => {
      const { fixture, el, component, releasesStub } = setup('maintainer');
      releasesStub.uploadAsset.mockReturnValue(NEVER);

      component.onFilesSelected([new File(['x'], 'big.iso')]);
      fixture.detectChanges();

      expect(text(el.querySelector('.release-detail__uploading'))).toBe('Envoi de big.iso…');
    });
  });

  describe('aside panels', () => {
    it('links the tag to the repository files at that tag', () => {
      const { el } = setup('reader');
      const tag = panel(el, 'Tag');
      expect(text(tag.querySelector('gbt-badge'))).toBe('v1.0.0');
      expect(tag.querySelector('a')!.getAttribute('href')).toBe('/repositories/alice/hello/-/tree/v1.0.0');
    });

    it('shows the short target commit, the full sha on hover', () => {
      const sha = 'abc123def4567890abc123def4567890abc12345';
      const { el } = setup('reader', { ...baseRelease, targetCommitSha: sha });
      const commit = panel(el, 'Commit cible').querySelector('.gbt-badge__label')!;
      expect(text(commit)).toBe('abc123d');
      expect(commit.getAttribute('title')).toBe(sha);
    });

    it('shows a missing-tag notice when targetCommitSha is null', () => {
      const { fixture } = setup('reader', { ...baseRelease, targetCommitSha: null });
      expect(fixture.nativeElement.textContent).toContain('supprimé');
    });

    it('warns in the commit panel that the tag was deleted, and drops the link to its files', () => {
      const { el } = setup('reader', { ...baseRelease, targetCommitSha: null });
      const commit = panel(el, 'Commit cible');
      expect(text(commit.querySelector('.release-detail__missing-tag'))).toBe("Le tag git de cette release a été supprimé ; le commit cible n'est plus disponible.");
      expect(commit.querySelector('.release-detail__missing-tag gbt-icon')).toBeTruthy();
      // A lasting fact about the page, not an interruption: no assertive live region.
      expect(commit.querySelector('[role="alert"]')).toBeNull();
      expect(panel(el, 'Tag').querySelector('a')).toBeNull();
    });

    it('shows the author as a user chip', () => {
      const { el } = setup('reader', { ...baseRelease, author: { id: 'a', username: 'alice' } });
      expect(text(panel(el, 'Auteur').querySelector('gbt-user-chip'))).toBe('alice');
    });

    it('says the author was deleted when the user no longer resolves', () => {
      const { el } = setup('reader');
      expect(panel(el, 'Auteur').querySelector('gbt-user-chip')).toBeNull();
      expect(text(panel(el, 'Auteur').querySelector('.release-detail__empty'))).toBe('Utilisateur supprimé');
    });

    it('dates the creation and the publication', () => {
      const { el } = setup('reader', { ...baseRelease, createdAt: '2025-12-30T00:00:00Z' });
      expect(facts(panel(el, 'Publication'))).toEqual([
        ['Créée', relativeTime('2025-12-30T00:00:00Z')],
        ['Publiée', relativeTime(baseRelease.publishedAt!)],
      ]);
      expect(panel(el, 'Publication').querySelector('time')!.getAttribute('title')).toBe(absoluteDateTime('2025-12-30T00:00:00Z'));
      expect(panel(el, 'Publication').querySelector('dl')!.getAttribute('data-value-align')).toBe('end');
    });

    it('says a draft is not published yet', () => {
      const { el } = setup('maintainer', { ...baseRelease, draft: true, publishedAt: null });
      expect(facts(panel(el, 'Publication'))[1]).toEqual(['Publiée', 'Pas encore publiée']);
    });

    it('sums up the files: count and total size', () => {
      const { el } = setup('reader', {
        ...baseRelease,
        assets: [
          { ...asset, id: 'a1', sizeBytes: 1024 * 1024 },
          { ...asset, id: 'a2', sizeBytes: 1024 * 1024 },
        ],
      });
      expect(facts(panel(el, 'Téléchargements'))).toEqual([
        ['Fichiers', '2'],
        ['Taille totale', '2 Mo'],
      ]);
      expect(panel(el, 'Téléchargements').querySelector('dl')!.getAttribute('data-value-align')).toBe('end');
    });
  });

  describe('loading and not found', () => {
    it('shows the page’s shape while loading', () => {
      const { el } = setup('reader', baseRelease, () => NEVER);
      expect(el.querySelector('.release-detail__loading')!.getAttribute('aria-busy')).toBe('true');
      expect(el.querySelector('gbt-page-header')).toBeNull();
    });

    it('says the release was not found, with a way back to the releases', () => {
      const { el } = setup('reader', baseRelease, () => throwError(() => new Error('404')));
      expect(text(el.querySelector('.release-detail__not-found'))).toContain('Release introuvable');
      expect(el.querySelector('.release-detail__not-found a')!.getAttribute('href')).toBe('/repositories/alice/hello/-/releases');
    });
  });
});
