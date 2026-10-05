import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { Observable, of, Subject, throwError } from 'rxjs';
import { formatDateTime } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { ApiTokensList } from './api-tokens-list';
import { ApiTokenSummary, TokensService } from '../api-tokens.service';

const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

const TOKENS: ApiTokenSummary[] = [
  { id: 't1', name: 'ci-github-actions', createdAt: '2026-08-12T09:30:00Z', lastUsedAt: minutesAgo(7) },
  { id: 't2', name: 'script-de-sauvegarde', createdAt: '2026-09-20T11:00:00Z', lastUsedAt: null },
];

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('ApiTokensList', () => {
  function setup(tokens: ApiTokenSummary[] | Observable<ApiTokenSummary[]> = []) {
    const tokensStub = {
      list: vi.fn((): Observable<ApiTokenSummary[]> => (Array.isArray(tokens) ? of(tokens) : tokens)),
      create: vi.fn(() => of({ id: 't9', name: 'ci', token: 'plain-token' })),
      revoke: vi.fn(() => of<void>(undefined)),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: TokensService, useValue: tokensStub },
        { provide: GbtToastService, useValue: toastStub },
        { provide: LOCALE_ID, useValue: 'fr' },
      ],
    });
    const fixture = TestBed.createComponent(ApiTokensList);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const doc = () => el.ownerDocument;
    const card = (heading: string) => Array.from(el.querySelectorAll('gbt-card')).find((c) => text(c.querySelector('h2'))?.startsWith(heading));
    const button = (root: ParentNode, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label || b.getAttribute('aria-label') === label);
    const nameInput = () => el.querySelector<HTMLInputElement>('.api-tokens-list__name input')!;
    const typeName = (value: string) => {
      nameInput().value = value;
      nameInput().dispatchEvent(new Event('input'));
      fixture.detectChanges();
    };
    const dialog = () => doc().querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');
    return { fixture, el, component: fixture.componentInstance, tokensStub, toastStub, card, button, nameInput, typeName, dialog };
  }

  describe('the list', () => {
    it('says a token is for Git over HTTPS, not for the REST API', () => {
      const { el } = setup();

      const intro = text(el.querySelector('gbt-card'));
      expect(intro).toContain('Un jeton remplace votre mot de passe pour Git en HTTPS : cloner, récupérer et pousser.');
      expect(intro).toContain("Il ne donne pas accès à l'API REST.");
      expect(intro).not.toContain("pour l'API");
    });

    it('lists the tokens as rows in a card counting them, under a creation card', () => {
      const { el, card } = setup(TOKENS);

      expect(Array.from(el.querySelectorAll('gbt-card h2')).map(text)).toEqual(['Nouveau jeton', 'Jetons actifs 2']);
      const rows = card('Jetons actifs')!.querySelectorAll('ul li gbt-list-row');
      expect(rows).toHaveLength(2);
      expect(Array.from(rows).map((row) => text(row.querySelector('.api-tokens-list__token-name')))).toEqual(['ci-github-actions', 'script-de-sauvegarde']);
    });

    it('wears a key icon on the list card', () => {
      const { fixture } = setup(TOKENS);

      const list = fixture.debugElement.queryAll(By.css('gbt-card')).find((de) => text(de.nativeElement.querySelector('h2'))?.startsWith('Jetons actifs'))!;
      expect((list.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name()).toBe('key');
    });

    it('shows when each token was created and last used, relative with the exact date on hover', () => {
      const { el } = setup(TOKENS);

      const [used, unused] = Array.from(el.querySelectorAll('gbt-list-row'));
      const created = used.querySelector('.api-tokens-list__created time')!;
      expect(created.getAttribute('datetime')).toBe(TOKENS[0].createdAt);
      expect(created.getAttribute('title')).toBe(absoluteDateTime(TOKENS[0].createdAt));
      expect(text(used.querySelector('.api-tokens-list__created'))).toBe('Créé le 12/08/2026');
      const lastUsed = used.querySelector('.api-tokens-list__last-used time')!;
      expect(lastUsed.getAttribute('datetime')).toBe(TOKENS[0].lastUsedAt);
      expect(lastUsed.getAttribute('title')).toBe(absoluteDateTime(TOKENS[0].lastUsedAt!));
      expect(text(used.querySelector('.api-tokens-list__last-used'))).toBe('Utilisé il y a 7 min');

      expect(text(unused.querySelector('.api-tokens-list__last-used'))).toBe('Jamais utilisé');
      expect(unused.querySelector('.api-tokens-list__last-used time')).toBeNull();
    });

    it('shows an empty state when there are no tokens', () => {
      const { el, card } = setup([]);

      expect(text(card('Jetons actifs')?.querySelector('h2'))).toBe('Jetons actifs 0');
      expect(text(el.querySelector('gbt-empty-state .gbt-empty-state__heading'))).toBe('Aucun jeton');
      expect(el.querySelector('gbt-empty-state .gbt-empty-state__message')).not.toBeNull();
      expect(el.querySelector('gbt-list-row')).toBeNull();
    });

    it('shows a loading state until the first list arrives', () => {
      const pending = new Subject<ApiTokenSummary[]>();
      const { fixture, el } = setup(pending);

      expect(el.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(2);
      // One polite status, outside any aria-busy region (a busy ancestor can hold back the announcement).
      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des jetons…');
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      pending.next(TOKENS);
      fixture.detectChanges();
      expect(el.querySelector('gbt-skeleton-list')).toBeNull();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it('keeps the list shown while it is refreshed', () => {
      const { fixture, el, tokensStub } = setup(TOKENS);

      tokensStub.list.mockReturnValue(new Subject<ApiTokenSummary[]>());
      fixture.componentInstance.refresh();
      fixture.detectChanges();

      expect(el.querySelector('gbt-skeleton-list')).toBeNull();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it('shows a failed state with a retry button when loading fails; retrying loads again', () => {
      const { fixture, el, tokensStub, toastStub, button } = setup(throwError(() => ({ status: 500 })));

      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les jetons n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      // The error toast announces the failure and the inline block stays silent: one live region, not two.
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les jetons. Réessayez plus tard.', 'error');

      tokensStub.list.mockReturnValue(of(TOKENS));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();
      expect(tokensStub.list).toHaveBeenCalledTimes(2);
      expect(el.querySelector('gbt-alert .gbt-alert[data-variant="error"]')).toBeNull();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });
  });

  describe('generating a token', () => {
    it('offers a named field and a "Générer" button, disabled while the name is blank', () => {
      const { el, button, typeName } = setup();

      const create = el.querySelector('gbt-card')!;
      expect(text(create.querySelector('gbt-input label'))).toBe('Nom du jeton');
      expect(button(create, 'Générer')!.disabled).toBe(true);
      typeName('   ');
      expect(button(create, 'Générer')!.disabled).toBe(true);
      typeName('ci');
      expect(button(create, 'Générer')!.disabled).toBe(false);
    });

    it('does not create a token with a blank name', () => {
      const { component, tokensStub } = setup();

      component['newTokenName'].set('  ');
      component.create();

      expect(tokensStub.create).not.toHaveBeenCalled();
    });

    it('creates the token, clears the field, refreshes the list and reveals the token once in its own card', () => {
      const { el, fixture, tokensStub, toastStub, typeName, card } = setup(TOKENS);
      tokensStub.create.mockReturnValue(of({ id: 't9', name: 'pipeline-de-release', token: 'fgt_plain-token' }));

      typeName('  pipeline-de-release ');
      el.querySelector('form.api-tokens-list__form')!.dispatchEvent(new Event('submit', { cancelable: true }));
      fixture.detectChanges();

      expect(tokensStub.create).toHaveBeenCalledExactlyOnceWith('pipeline-de-release');
      expect(fixture.componentInstance['newTokenName']()).toBe('');
      expect(tokensStub.list).toHaveBeenCalledTimes(2);
      expect(toastStub.show).toHaveBeenCalledWith('Jeton généré.');

      const revealed = el.querySelector('.api-tokens-list__revealed')!;
      expect(revealed).not.toBeNull();
      expect(text(revealed.querySelector('h2'))).toBe('Jeton « pipeline-de-release » généré');
      expect(revealed.querySelector(':scope > .gbt-card__header')?.getAttribute('data-tone')).toBe('success');
      expect(el.querySelector('gbt-card:not(.api-tokens-list__revealed) > .gbt-card__header')?.hasAttribute('data-tone')).toBe(false);
      expect(text(revealed.querySelector('gbt-copy-field code'))).toBe('fgt_plain-token');
      expect(text(revealed.querySelector('.api-tokens-list__warning'))).toContain('ne sera plus jamais affiché');
      expect(revealed.querySelector('.api-tokens-list__warning [role="alert"]')).not.toBeNull();
      expect(revealed.querySelector('.api-tokens-list__warning gbt-icon')).not.toBeNull();
      const headings = Array.from(el.querySelectorAll('gbt-card h2')).map(text);
      expect(headings.indexOf('Jeton « pipeline-de-release » généré')).toBe(1);
      expect(card('Jetons actifs')).toBeDefined();
    });

    it('marks the new token\'s row as new', () => {
      const { component, fixture, el, tokensStub } = setup(TOKENS);
      tokensStub.create.mockReturnValue(of({ id: 't2', name: 'script-de-sauvegarde', token: 'fgt_plain-token' }));

      component['newTokenName'].set('script-de-sauvegarde');
      component.create();
      fixture.detectChanges();

      const rows = Array.from(el.querySelectorAll('gbt-list-row'));
      expect(text(rows[1].querySelector('gbt-badge'))).toBe('Nouveau');
      expect(rows[0].querySelector('gbt-badge')).toBeNull();
    });

    describe('copy', () => {
      let savedClipboard: PropertyDescriptor | undefined;
      let writeText: ReturnType<typeof vi.fn>;

      beforeEach(() => {
        savedClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
        writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true, writable: true });
      });

      afterEach(() => {
        if (savedClipboard) {
          Object.defineProperty(navigator, 'clipboard', savedClipboard);
        } else {
          delete (navigator as { clipboard?: Clipboard }).clipboard;
        }
      });

      it('copies the revealed token with a named copy button', async () => {
        const { component, fixture, el, tokensStub } = setup();
        tokensStub.create.mockReturnValue(of({ id: 't9', name: 'ci', token: 'fgt_plain-token' }));
        component['newTokenName'].set('ci');
        component.create();
        fixture.detectChanges();

        el.querySelector<HTMLButtonElement>('.api-tokens-list__revealed [aria-label="Copier le jeton"]')!.click();
        await fixture.whenStable();
        fixture.detectChanges();

        expect(writeText).toHaveBeenCalledExactlyOnceWith('fgt_plain-token');
        expect(text(el.querySelector('.api-tokens-list__revealed gbt-copy-field [role="status"]'))).toBe('Copié');
      });
    });

    it('hides the token for good once "J\'ai copié le jeton" is clicked', () => {
      const { component, fixture, el, button } = setup();
      component['newTokenName'].set('ci');
      component.create();
      fixture.detectChanges();

      button(el, "J'ai copié le jeton")!.click();
      fixture.detectChanges();

      expect(el.querySelector('.api-tokens-list__revealed')).toBeNull();
      expect(el.textContent).not.toContain('plain-token');
    });

    it('surfaces an error and keeps the name when creating a token fails', () => {
      const { component, fixture, tokensStub, toastStub, el } = setup();
      tokensStub.create.mockReturnValue(throwError(() => new Error('network')));
      component['newTokenName'].set('ci');

      component.create();
      fixture.detectChanges();

      expect(toastStub.show).toHaveBeenCalledWith('Impossible de générer le jeton.', 'error');
      expect(component['newTokenName']()).toBe('ci');
      expect(el.querySelector('.api-tokens-list__revealed')).toBeNull();
    });

    it('sends one creation while it runs', () => {
      const { component, tokensStub } = setup();
      tokensStub.create.mockReturnValue(new Subject());
      component['newTokenName'].set('ci');

      component.create();
      component.create();

      expect(tokensStub.create).toHaveBeenCalledOnce();
    });
  });

  describe('focus', () => {
    // A card takes focus on its heading (an h2 with tabindex -1), so a screen reader reads the heading, then the card's content.

    it('moves the focus to the reveal card, named by its heading, once a token is generated', async () => {
      const { fixture, el, tokensStub, typeName, button } = setup(TOKENS);
      tokensStub.create.mockReturnValue(of({ id: 't9', name: 'pipeline-de-release', token: 'fgt_plain-token' }));
      typeName('pipeline-de-release');
      const submit = button(el.querySelector('gbt-card')!, 'Générer')!;
      submit.focus();
      expect(document.activeElement).toBe(submit);

      el.querySelector('form.api-tokens-list__form')!.dispatchEvent(new Event('submit', { cancelable: true }));
      fixture.detectChanges();
      await fixture.whenStable();

      const card = el.querySelector('.api-tokens-list__revealed')!;
      const heading = card.querySelector('h2')!;
      expect(document.activeElement).toBe(heading);
      expect(heading.getAttribute('tabindex')).toBe('-1');
      expect(text(heading)).toBe('Jeton « pipeline-de-release » généré');
      expect(card.contains(card.querySelector('.api-tokens-list__warning'))).toBe(true);
      expect(card.querySelector('[aria-label="Copier le jeton"]')).not.toBeNull();
    });

    it('gives the focus back to the name field when the reveal is dismissed', async () => {
      const { fixture, el, component, button } = setup();
      component['newTokenName'].set('ci');
      component.create();
      fixture.detectChanges();
      await fixture.whenStable();

      button(el, "J'ai copié le jeton")!.click();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(el.querySelector('.api-tokens-list__revealed')).toBeNull();
      expect(document.activeElement).toBe(el.querySelector('.api-tokens-list__name input'));
    });

    it('leaves the focus alone when generating fails', async () => {
      const { fixture, el, tokensStub, typeName, nameInput } = setup();
      const refused = new Subject<{ id: string; name: string; token: string }>();
      tokensStub.create.mockReturnValue(refused);
      typeName('ci');
      nameInput().focus();

      el.querySelector('form.api-tokens-list__form')!.dispatchEvent(new Event('submit', { cancelable: true }));
      fixture.detectChanges();
      refused.error({ status: 500 });
      fixture.detectChanges();
      await fixture.whenStable();

      expect(document.activeElement).toBe(nameInput());
    });

    it('keeps the focus in the list after a revoke: on the next row\'s revoke button', async () => {
      const { fixture, el, tokensStub, button, dialog, card } = setup(TOKENS);
      const trigger = button(el, 'Révoquer le jeton ci-github-actions')!;
      trigger.focus();
      trigger.click();
      fixture.detectChanges();
      await fixture.whenStable();

      tokensStub.list.mockReturnValue(of([TOKENS[1]]));
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(1);
      expect(document.activeElement).toBe(button(el, 'Révoquer le jeton script-de-sauvegarde'));
      expect(card('Jetons actifs')!.contains(document.activeElement)).toBe(true);
    });

    it('keeps the focus in the list card after revoking the last token', async () => {
      const { fixture, el, tokensStub, button, dialog, card } = setup([TOKENS[0]]);
      const trigger = button(el, 'Révoquer le jeton ci-github-actions')!;
      trigger.focus();
      trigger.click();
      fixture.detectChanges();
      await fixture.whenStable();

      tokensStub.list.mockReturnValue(of([]));
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();
      await fixture.whenStable();

      const heading = card('Jetons actifs')!.querySelector('h2')!;
      expect(document.activeElement).toBe(heading);
      expect(heading.getAttribute('tabindex')).toBe('-1');
      expect(text(heading)).toBe('Jetons actifs 0');
    });

    it('does not move the focus when the list refreshes without a revoke', async () => {
      const { fixture, component, nameInput } = setup(TOKENS);
      nameInput().focus();

      component.refresh();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(document.activeElement).toBe(nameInput());
    });
  });

  describe('revoking a token', () => {
    it('asks for a light confirmation naming the token, and revokes it once confirmed', () => {
      const { el, fixture, tokensStub, toastStub, button, dialog } = setup(TOKENS);

      button(el, 'Révoquer le jeton ci-github-actions')!.click();
      fixture.detectChanges();

      expect(tokensStub.revoke).not.toHaveBeenCalled();
      expect(dialog()?.getAttribute('aria-label')).toBe('Révoquer le jeton');
      expect(text(dialog())).toContain('« ci-github-actions »');
      expect(dialog()!.querySelector('input')).toBeNull();

      tokensStub.list.mockReturnValue(of([TOKENS[1]]));
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();

      expect(tokensStub.revoke).toHaveBeenCalledExactlyOnceWith('t1');
      expect(toastStub.show).toHaveBeenCalledWith('Jeton révoqué.');
      expect(tokensStub.list).toHaveBeenCalledTimes(2);
      expect(dialog()).toBeNull();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(1);
    });

    it('sends one DELETE even when the confirmation is clicked twice while it runs', () => {
      const { el, fixture, tokensStub, button, dialog } = setup(TOKENS);
      const pending = new Subject<void>();
      tokensStub.revoke.mockReturnValue(pending);

      button(el, 'Révoquer le jeton ci-github-actions')!.click();
      fixture.detectChanges();
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();
      button(dialog()!, 'Révoquer')?.click();
      fixture.detectChanges();

      expect(tokensStub.revoke).toHaveBeenCalledOnce();
      expect(dialog()).not.toBeNull();

      pending.next();
      pending.complete();
      fixture.detectChanges();
      expect(dialog()).toBeNull();
    });

    it('revokes nothing when the confirmation is cancelled', () => {
      const { el, fixture, tokensStub, button, dialog } = setup(TOKENS);

      button(el, 'Révoquer le jeton ci-github-actions')!.click();
      fixture.detectChanges();
      button(dialog()!, 'Annuler')!.click();
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      expect(tokensStub.revoke).not.toHaveBeenCalled();
    });

    it('shows a toast and refreshes the list when a token is revoked', () => {
      const { component, tokensStub, toastStub } = setup();
      tokensStub.list.mockReturnValue(of([{ id: 't1', name: 'ci', createdAt: '2026-01-01', lastUsedAt: null }]));

      component.revoke('t1');

      expect(tokensStub.revoke).toHaveBeenCalledWith('t1');
      expect(toastStub.show).toHaveBeenCalledWith('Jeton révoqué.');
      expect(tokensStub.list).toHaveBeenCalledTimes(2);
    });

    it('surfaces an error and closes the confirmation when revoking a token fails', () => {
      const { el, fixture, toastStub, tokensStub, button, dialog } = setup(TOKENS);
      const refused = new Subject<void>();
      tokensStub.revoke.mockReturnValue(refused);

      button(el, 'Révoquer le jeton ci-github-actions')!.click();
      fixture.detectChanges();
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();
      refused.error(new Error('network'));
      fixture.detectChanges();

      expect(toastStub.show).toHaveBeenCalledWith('Impossible de révoquer ce jeton.', 'error');
      expect(dialog()).toBeNull();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it('drops the reveal when the revealed token is revoked', () => {
      const { component, fixture, el, tokensStub } = setup(TOKENS);
      tokensStub.create.mockReturnValue(of({ id: 't2', name: 'script-de-sauvegarde', token: 'fgt_plain-token' }));
      component['newTokenName'].set('script-de-sauvegarde');
      component.create();
      fixture.detectChanges();
      expect(el.querySelector('.api-tokens-list__revealed')).not.toBeNull();

      component.revoke('t1');
      fixture.detectChanges();
      expect(el.querySelector('.api-tokens-list__revealed')).not.toBeNull();

      component.revoke('t2');
      fixture.detectChanges();
      expect(el.querySelector('.api-tokens-list__revealed')).toBeNull();
    });
  });
});
