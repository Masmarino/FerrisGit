import { LOCALE_ID, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Observable, of, Subject, throwError } from 'rxjs';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AccessTokensList } from './access-tokens-list';
import { AccessTokensService, AccessTokenSummary } from '../access-tokens.service';
import { MeService } from '../../shell/me.service';

const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

const TOKENS: AccessTokenSummary[] = [
  { id: 't1', name: 'fgit-portable', scopes: ['repositories:read', 'merge-requests:write'], expiresAt: '2027-02-01T10:00:00Z', expired: false, createdAt: '2026-11-01T10:00:00Z', lastUsedAt: minutesAgo(7) },
  { id: 't2', name: 'vieux-script', scopes: ['issues:read'], expiresAt: '2026-10-01T10:00:00Z', expired: true, createdAt: '2026-09-01T10:00:00Z', lastUsedAt: null },
];

const CREATED = { ...TOKENS[0], id: 't9', name: 'ci', token: 'fga_plain-token' };

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('AccessTokensList', () => {
  function setup(tokens: AccessTokenSummary[] | Observable<AccessTokenSummary[]> = [], options: { isAdmin?: boolean } = {}) {
    const tokensStub = {
      list: vi.fn((): Observable<AccessTokenSummary[]> => (Array.isArray(tokens) ? of(tokens) : tokens)),
      create: vi.fn((): Observable<typeof CREATED> => of(CREATED)),
      revoke: vi.fn(() => of<void>(undefined)),
    };
    const toastStub = { show: vi.fn() };
    const isAdmin = signal(options.isAdmin ?? false);
    TestBed.configureTestingModule({
      providers: [
        { provide: AccessTokensService, useValue: tokensStub },
        { provide: GbtToastService, useValue: toastStub },
        { provide: MeService, useValue: { isAdmin } },
        { provide: LOCALE_ID, useValue: 'fr' },
      ],
    });
    const fixture = TestBed.createComponent(AccessTokensList);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const button = (root: ParentNode, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label || b.getAttribute('aria-label') === label);
    const typeName = (value: string) => {
      const input = el.querySelector<HTMLInputElement>('.access-tokens-list__name input')!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();
    };
    const choose = (area: string, level: string) => {
      button(el.querySelector(`[data-area="${area}"]`)!, level)!.click();
      fixture.detectChanges();
    };
    const lifetime = (label: string) => {
      button(el.querySelector('.access-tokens-list__lifetime')!, label)!.click();
      fixture.detectChanges();
    };
    const typeDays = async (value: string) => {
      await fixture.whenStable(); // ngModel writes a field's value asynchronously
      const input = el.querySelector<HTMLInputElement>('.access-tokens-list__custom-lifetime input')!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();
    };
    const submit = () => {
      el.querySelector('form.access-tokens-list__form')!.dispatchEvent(new Event('submit', { cancelable: true }));
      fixture.detectChanges();
    };
    const generate = () => button(el, 'Générer')!;
    const dialog = () => el.ownerDocument.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');
    return { fixture, el, component: fixture.componentInstance, tokensStub, toastStub, isAdmin, button, typeName, choose, lifetime, typeDays, submit, generate, dialog };
  }

  describe('the form', () => {
    it('offers read, none or write on each area, and no admin to someone who is not one', () => {
      const { el } = setup();

      const areas = Array.from(el.querySelectorAll('[data-area]')).map((row) => row.getAttribute('data-area'));
      expect(areas).toEqual(['repositories', 'issues', 'merge-requests', 'pipelines', 'releases']);
      const options = Array.from(el.querySelectorAll('[data-area="issues"] [role="radio"]')).map(text);
      expect(options).toEqual(['Aucun', 'Lecture', 'Écriture']);
      expect(el.querySelector('[data-area="issues"] [aria-checked="true"]')?.textContent?.trim()).toBe('Aucun');
    });

    it('names each choice after its area for a screen reader', () => {
      const { el } = setup();

      expect(el.querySelector('[data-area="merge-requests"] [role="radiogroup"]')?.getAttribute('aria-label')).toBe('Demandes de fusion');
    });

    it('offers the admin area to an administrator, also when that is only known later', () => {
      const { el, fixture, isAdmin } = setup();
      expect(el.querySelector('[data-area="admin"]')).toBeNull();

      isAdmin.set(true);
      fixture.detectChanges();

      expect(el.querySelector('[data-area="admin"]')).not.toBeNull();
    });

    it('stays disabled until there is a name and at least one scope', () => {
      const { el, generate, typeName, choose, submit, tokensStub } = setup();

      expect(generate().disabled).toBe(true);
      expect(text(el.querySelector('.access-tokens-list__summary'))).toBe('Choisissez au moins une portée.');
      typeName('ci');
      expect(generate().disabled).toBe(true);
      submit();
      expect(tokensStub.create).not.toHaveBeenCalled();

      choose('issues', 'Lecture');
      expect(generate().disabled).toBe(false);
      expect(el.querySelector('.access-tokens-list__summary')).toBeNull();

      typeName('   ');
      expect(generate().disabled).toBe(true);
    });

    it('sends one scope per area as the server spells them, with the default lifetime', () => {
      const { typeName, choose, submit, tokensStub } = setup();

      typeName('  ci  ');
      choose('repositories', 'Lecture');
      choose('merge-requests', 'Écriture');
      choose('issues', 'Lecture');
      choose('issues', 'Aucun');
      submit();

      expect(tokensStub.create).toHaveBeenCalledExactlyOnceWith({ name: 'ci', scopes: ['repositories:read', 'merge-requests:write'], expiresInDays: 90 });
    });

    it('sends the admin scope only from an administrator', () => {
      const { typeName, choose, submit, tokensStub, isAdmin, fixture } = setup([], { isAdmin: true });

      typeName('ci');
      choose('admin', 'Écriture');
      choose('issues', 'Lecture');
      isAdmin.set(false); // no longer one by the time the form is sent
      fixture.detectChanges();
      submit();

      expect(tokensStub.create).toHaveBeenCalledExactlyOnceWith({ name: 'ci', scopes: ['issues:read'], expiresInDays: 90 });
    });

    it('sends the preset lifetime that was chosen', () => {
      const { typeName, choose, lifetime, submit, tokensStub } = setup();

      typeName('ci');
      choose('issues', 'Lecture');
      lifetime('365 jours');
      submit();

      expect(tokensStub.create).toHaveBeenCalledExactlyOnceWith({ name: 'ci', scopes: ['issues:read'], expiresInDays: 365 });
    });

    it('takes a custom lifetime from 1 to 365 days, whole, and says so otherwise', async () => {
      const { el, typeName, choose, lifetime, typeDays, submit, generate, tokensStub } = setup();
      typeName('ci');
      choose('issues', 'Lecture');
      lifetime('Autre');
      expect(generate().disabled).toBe(true); // nothing typed yet, and nothing to scold
      expect(text(el.querySelector('.access-tokens-list__custom-lifetime'))).not.toContain('Indiquez');

      for (const refused of ['0', '366', '1.5', '-3']) {
        await typeDays(refused);
        expect(generate().disabled, refused).toBe(true);
        expect(text(el.querySelector('.access-tokens-list__custom-lifetime')), refused).toContain('Indiquez un nombre entier de jours, de 1 à 365.');
      }

      await typeDays('12');
      expect(generate().disabled).toBe(false);
      submit();
      expect(tokensStub.create).toHaveBeenCalledExactlyOnceWith({ name: 'ci', scopes: ['issues:read'], expiresInDays: 12 });
    });

    it('refuses a name the server would refuse, before asking it', () => {
      const { el, typeName, choose, generate } = setup();
      choose('issues', 'Lecture');

      typeName('a'.repeat(101));

      expect(generate().disabled).toBe(true);
      expect(text(el.querySelector('.access-tokens-list__name'))).toContain('Le nom tient en 100 caractères au plus');

      typeName('a'.repeat(100));
      expect(generate().disabled).toBe(false);
    });

    it('sends one creation while it runs', () => {
      const { component, typeName, choose, tokensStub } = setup();
      tokensStub.create.mockReturnValue(new Subject());
      typeName('ci');
      choose('issues', 'Lecture');

      component.create();
      component.create();

      expect(tokensStub.create).toHaveBeenCalledOnce();
    });

    it('keeps what was chosen and says so when creating fails', () => {
      const { el, fixture, typeName, choose, submit, tokensStub, toastStub, generate } = setup();
      tokensStub.create.mockReturnValue(throwError(() => ({ status: 400 })));
      typeName('ci');
      choose('issues', 'Lecture');

      submit();
      fixture.detectChanges();

      expect(toastStub.show).toHaveBeenCalledWith('Impossible de générer le jeton.', 'error');
      expect(el.querySelector('.access-tokens-list__revealed')).toBeNull();
      expect(generate().disabled).toBe(false);
    });
  });

  describe('the one-time reveal', () => {
    function created() {
      const ctx = setup(TOKENS);
      ctx.typeName('ci');
      ctx.choose('issues', 'Lecture');
      ctx.submit();
      ctx.fixture.detectChanges();
      return ctx;
    }

    it('shows the token once in its own card, empties the form and refreshes the list', () => {
      const { el, tokensStub, toastStub, generate, component } = created();

      const revealed = el.querySelector('.access-tokens-list__revealed')!;
      expect(text(revealed.querySelector('h2'))).toBe('Jeton « ci » généré');
      expect(text(revealed.querySelector('gbt-copy-field code'))).toBe('fga_plain-token');
      expect(text(revealed.querySelector('.access-tokens-list__warning'))).toContain('ne sera plus jamais affiché');
      expect(toastStub.show).toHaveBeenCalledWith('Jeton généré.');
      expect(tokensStub.list).toHaveBeenCalledTimes(2);
      expect(component['newTokenName']()).toBe('');
      expect(el.querySelector('[data-area="issues"] [aria-checked="true"]')?.textContent?.trim()).toBe('Aucun');
      expect(generate().disabled).toBe(true);
    });

    it('moves the focus to the reveal, since emptying the form disables the button', async () => {
      const { el, fixture } = created();

      await fixture.whenStable();

      expect(document.activeElement).toBe(el.querySelector('.access-tokens-list__revealed h2'));
    });

    it('hides the token for good once acknowledged, with the focus back on the name', async () => {
      const { el, fixture, button } = created();

      button(el, "J'ai copié le jeton")!.click();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(el.querySelector('.access-tokens-list__revealed')).toBeNull();
      expect(el.textContent).not.toContain('fga_plain-token');
      expect(document.activeElement).toBe(el.querySelector('.access-tokens-list__name input'));
    });

    it('drops the reveal when the revealed token is revoked', () => {
      const { component, fixture, el } = created();

      component.revoke('t9');
      fixture.detectChanges();

      expect(el.querySelector('.access-tokens-list__revealed')).toBeNull();
    });
  });

  describe('the list', () => {
    it('lists each token with its scopes in plain words, its expiry and its last use', () => {
      const { el } = setup(TOKENS);

      const [first, second] = Array.from(el.querySelectorAll('gbt-list-row'));
      expect(text(first.querySelector('.access-tokens-list__token-name'))).toBe('fgit-portable');
      expect(Array.from(first.querySelectorAll('.access-tokens-list__scope')).map(text)).toEqual(['Dépôts : lecture', 'Demandes de fusion : lecture et écriture']);
      expect(first.querySelector('.access-tokens-list__scope')?.getAttribute('title')).toBe('repositories:read');
      expect(text(first.querySelector('.access-tokens-list__expiry'))).toBe('Expire le 01/02/2027');
      expect(first.querySelector('.access-tokens-list__expired-badge')).toBeNull();
      expect(text(first.querySelector('.access-tokens-list__last-used'))).toBe('Utilisé il y a 7 min');
      expect(text(second.querySelector('.access-tokens-list__last-used'))).toBe('Jamais utilisé');
    });

    it('shows an expired token as expired, and it can still be revoked', () => {
      const { el, button } = setup(TOKENS);

      const expired = Array.from(el.querySelectorAll('gbt-list-row'))[1];
      expect(text(expired.querySelector('.access-tokens-list__expired-badge'))).toBe('Expiré');
      expect(text(expired.querySelector('.access-tokens-list__expiry'))).toBe('Expiré le 01/10/2026');
      expect(button(expired, 'Révoquer le jeton vieux-script')).toBeDefined();
    });

    it('counts the tokens, with an empty state when there are none', () => {
      const { el, fixture, tokensStub, component } = setup([]);

      expect(el.querySelector('gbt-empty-state')).not.toBeNull();
      tokensStub.list.mockReturnValue(of(TOKENS));
      component.refresh();
      fixture.detectChanges();
      expect(text(Array.from(el.querySelectorAll('gbt-card h2')).at(-1))).toBe("Vos jetons d'API 2");
    });

    it('shows a loading state, then the list', () => {
      const pending = new Subject<AccessTokenSummary[]>();
      const { el, fixture } = setup(pending);

      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des jetons…');
      pending.next(TOKENS);
      fixture.detectChanges();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it('offers to retry when loading fails', () => {
      const { el, fixture, tokensStub, toastStub, button } = setup(throwError(() => ({ status: 500 })));

      expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les jetons. Réessayez plus tard.', 'error');
      tokensStub.list.mockReturnValue(of(TOKENS));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();

      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });
  });

  describe('revoking a token', () => {
    it('asks for confirmation naming the token, says it also ends Git access, and revokes once confirmed', () => {
      const { el, fixture, tokensStub, toastStub, button, dialog } = setup(TOKENS);

      button(el, 'Révoquer le jeton fgit-portable')!.click();
      fixture.detectChanges();

      expect(tokensStub.revoke).not.toHaveBeenCalled();
      expect(text(dialog())).toContain("Les programmes qui utilisent « fgit-portable » perdront aussitôt l'accès à l'API et à Git.");

      tokensStub.list.mockReturnValue(of([TOKENS[1]]));
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();

      expect(tokensStub.revoke).toHaveBeenCalledExactlyOnceWith('t1');
      expect(toastStub.show).toHaveBeenCalledWith('Jeton révoqué.');
      expect(dialog()).toBeNull();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(1);
    });

    it('revokes nothing when the confirmation is cancelled', () => {
      const { el, fixture, tokensStub, button, dialog } = setup(TOKENS);

      button(el, 'Révoquer le jeton fgit-portable')!.click();
      fixture.detectChanges();
      button(dialog()!, 'Annuler')!.click();
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      expect(tokensStub.revoke).not.toHaveBeenCalled();
    });

    it('sends one DELETE while it runs', () => {
      const { component, tokensStub } = setup(TOKENS);
      tokensStub.revoke.mockReturnValue(new Subject());

      component.revoke('t1');
      component.revoke('t1');

      expect(tokensStub.revoke).toHaveBeenCalledOnce();
    });

    it('surfaces an error and closes the confirmation when revoking fails', () => {
      const { el, fixture, tokensStub, toastStub, button, dialog } = setup(TOKENS);
      tokensStub.revoke.mockReturnValue(throwError(() => ({ status: 500 })));

      button(el, 'Révoquer le jeton fgit-portable')!.click();
      fixture.detectChanges();
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();

      expect(toastStub.show).toHaveBeenCalledWith('Impossible de révoquer ce jeton.', 'error');
      expect(dialog()).toBeNull();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it('keeps the focus in the list after a revoke', async () => {
      const { el, fixture, tokensStub, button, dialog } = setup(TOKENS);
      const trigger = button(el, 'Révoquer le jeton fgit-portable')!;
      trigger.focus();
      trigger.click();
      fixture.detectChanges();
      await fixture.whenStable();

      tokensStub.list.mockReturnValue(of([TOKENS[1]]));
      button(dialog()!, 'Révoquer')!.click();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(document.activeElement).toBe(button(el, 'Révoquer le jeton vieux-script'));
    });
  });
});
