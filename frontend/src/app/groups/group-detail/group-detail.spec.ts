import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { GroupDetail } from './group-detail';
import { CreateRepositoryModal } from '../../repositories/create-repository-modal/create-repository-modal';
import { PageTitleService } from '../../shell/page-title.service';

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

const CHILD = { id: 'child-1', parentGroupId: 'group-1', name: 'terraform-modules', description: 'Modules partagés', createdAt: '2026-01-01T00:00:00Z' };
const REPO = { id: 'repo-1', name: 'billing-service', description: '', owner: 'someone-else', role: 'reader', visibility: 'private', createdAt: '2026-01-01T00:00:00Z', path: ['acme', 'backend', 'billing-service'] };
const MEMBERS = [
  { userId: 'u1', username: 'alice', role: 'maintainer', createdAt: '2026-01-01T00:00:00Z' },
  { userId: 'u2', username: 'bob', role: 'contributor', createdAt: '2026-01-02T00:00:00Z' },
  { userId: 'u3', username: 'carol', role: 'reader', createdAt: '2026-01-03T00:00:00Z' },
  { userId: 'u4', username: 'dan', role: 'reader', createdAt: '2026-01-04T00:00:00Z' },
  { userId: 'u5', username: 'eve', role: 'reader', createdAt: '2026-01-05T00:00:00Z' },
  { userId: 'u6', username: 'fanny', role: 'reader', createdAt: '2026-01-06T00:00:00Z' },
];

describe('GroupDetail', () => {
  function setup(groupId: string, path: string[] = ['acme', 'backend'], role: string | null = null) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])],
    });
    const fixture = TestBed.createComponent(GroupDetail);
    fixture.componentRef.setInput('groupId', groupId);
    fixture.componentRef.setInput('path', path);
    fixture.componentRef.setInput('role', role);
    const http = TestBed.inject(HttpTestingController);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, http, el };
  }

  function loaded(role: string | null, options: { members?: unknown[] } = {}) {
    const ctx = setup('group-1', ['acme', 'backend'], role);
    ctx.fixture.detectChanges();
    ctx.http.expectOne('/api/groups/group-1/children').flush([CHILD]);
    ctx.http.expectOne('/api/groups/group-1/repositories').flush([REPO]);
    if (role) {
      ctx.http.expectOne('/api/groups/group-1/members').flush(options.members ?? MEMBERS);
    }
    ctx.fixture.detectChanges();
    return ctx;
  }

  const headerButtons = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button'));
  const headerButton = (el: HTMLElement, label: string) => headerButtons(el).find((b) => text(b) === label);
  const membersLink = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLAnchorElement>('.gbt-page-header__actions a.gbt-button')).find((a) => text(a) === 'Membres');

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('puts the group name in the shell header', () => {
    loaded(null);
    expect(TestBed.inject(PageTitleService).title()).toBe('backend');
  });

  it('builds subgroup links relative to the resolved path, not the child UUID', () => {
    const { fixture, http } = setup('group-1', ['acme', 'backend']);
    fixture.detectChanges();
    http.match(() => true).forEach((req) => req.flush([]));
    fixture.detectChanges();

    const component = fixture.componentInstance;
    expect(component['childLink']('terraform-modules')).toEqual(['/repositories', 'acme', 'backend', 'terraform-modules']);
  });

  it('renders subgroup and repository items with links built from the resolved path', () => {
    const { fixture, http } = setup('group-1', ['acme', 'backend']);
    fixture.detectChanges();

    http.expectOne('/api/groups/group-1/children').flush([
      { id: 'child-1', parentGroupId: 'group-1', name: 'terraform-modules', description: '', createdAt: '2026-01-01' },
    ]);
    http.expectOne('/api/groups/group-1/repositories').flush([
      { id: 'repo-1', name: 'billing-service', description: '', owner: 'someone-else', role: 'reader', visibility: 'private', createdAt: '2026-01-01', path: ['acme', 'backend', 'billing-service'] },
    ]);
    fixture.detectChanges();

    const links: HTMLAnchorElement[] = Array.from(fixture.nativeElement.querySelectorAll('fg-workspace-grid a'));
    const hrefs = links.map((a) => a.getAttribute('href'));
    expect(hrefs).toContain('/repositories/acme/backend/terraform-modules');
    expect(hrefs).toContain('/repositories/acme/backend/billing-service');
  });

  it('does not show the empty state while the children/repositories requests are still pending', () => {
    const { fixture, http } = setup('group-1');
    fixture.detectChanges();

    expect(fixture.componentInstance['loading']()).toBe(true);
    expect(fixture.nativeElement.querySelector('gbt-empty-state')).toBeNull();

    http.match(() => true).forEach((req) => req.flush([]));
    fixture.detectChanges();

    expect(fixture.componentInstance['loading']()).toBe(false);
    expect(fixture.nativeElement.querySelector('gbt-empty-state')).toBeTruthy();
  });

  it("titles the page with the group's name in the page header, without a breadcrumb of its own (the shell has one)", () => {
    const { el } = loaded(null);
    expect(Array.from(el.querySelectorAll('h1'), (h) => text(h))).toEqual(['backend']);
    expect(el.querySelector('nav[aria-label="Fil d\'Ariane"]')).toBeNull();
    expect(text(el.querySelector('.gbt-page-header__meta'))).toContain('1 sous-groupe · 1 dépôt');
  });

  it('titles the rows relative to the group, the full path on hover', () => {
    const { el } = loaded(null);
    const links = Array.from(el.querySelectorAll('.workspace-grid__items .gbt-list-row__title > a'));
    expect(links.map((a) => text(a))).toEqual(['terraform-modules', 'billing-service']);
    expect(links.map((a) => a.getAttribute('title'))).toEqual(['acme/backend/terraform-modules', 'acme/backend/billing-service']);
  });

  it("shows the subgroup's description on its row", () => {
    const { el } = loaded(null);
    expect(text(el.querySelector('.workspace-grid__items .workspace-grid__description'))).toBe('Modules partagés');
  });

  describe('actions by role', () => {
    it('gives a maintainer "Membres", "Nouveau sous-groupe" (secondary) and "Nouveau dépôt" (the one primary)', () => {
      const { el } = loaded('maintainer');

      expect(headerButtons(el).map((b) => text(b))).toEqual(['Nouveau sous-groupe', 'Nouveau dépôt']);
      expect(headerButton(el, 'Nouveau sous-groupe')!.classList).toContain('gbt-button--secondary');
      expect(Array.from(el.querySelectorAll('.gbt-button--primary'), (b) => text(b))).toEqual(['Nouveau dépôt']);
      const members = membersLink(el)!;
      expect(members.getAttribute('href')).toBe('/groups/group-1/members');
      expect(text(members)).toBe('Membres');
    });

    it('gives a reader only the "Membres" link', () => {
      const { el } = loaded('reader');
      expect(headerButtons(el)).toHaveLength(0);
      expect(membersLink(el)).not.toBeUndefined();
    });

    it('gives someone without a role no action at all', () => {
      const { el } = loaded(null);
      expect(el.querySelector('.gbt-page-header__actions')?.children.length ?? 0).toBe(0);
    });

    it("lets a maintainer manage the subgroups from their rows (the role is inherited down the tree)", () => {
      const { el } = loaded('maintainer');
      const groupRow = el.querySelector('.workspace-grid__items > li')!;
      expect(groupRow.querySelector('.gbt-menu__trigger')).not.toBeNull();
    });

    it('opens the repository creation dialog on this group', () => {
      const { fixture, http, el } = loaded('maintainer');
      headerButton(el, 'Nouveau dépôt')!.click();
      fixture.detectChanges();

      const modal = fixture.debugElement.query(By.directive(CreateRepositoryModal))!.componentInstance as CreateRepositoryModal;
      http.expectOne('/api/groups/writable').flush([{ id: 'group-1', path: 'acme/backend' }]);
      expect(modal.location()).toBe('acme/backend');
    });
  });

  describe('subgroup creation dialog', () => {
    // NgForm registers its ngModel controls, and writes their values, a microtask later.
    async function openDialog() {
      const ctx = loaded('maintainer');
      headerButton(ctx.el, 'Nouveau sous-groupe')!.click();
      ctx.fixture.detectChanges();
      await ctx.fixture.whenStable();
      ctx.fixture.detectChanges();
      return ctx;
    }
    const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]');
    function typeName(fixture: { detectChanges(): void }, value: string) {
      const input = dialog()!.querySelector<HTMLInputElement>('.group-detail__create-name input')!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();
    }
    const submitButton = () => Array.from(dialog()!.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Créer le sous-groupe')!;

    it('opens a dialog (no inline form) with the name and description fields', async () => {
      const { el } = await openDialog();
      expect(dialog()).not.toBeNull();
      expect(text(dialog()!.querySelector('h2'))).toBe('Nouveau sous-groupe');
      expect(dialog()!.querySelector('.group-detail__create-description textarea')).not.toBeNull();
      expect(el.querySelector('.group-detail__create-form')).toBeNull();
    });

    it('describes the name field by its rules, so a screen reader reads them with the field', async () => {
      await openDialog();
      const input = dialog()!.querySelector<HTMLInputElement>('.group-detail__create-name input')!;
      const hint = dialog()!.querySelector(`#${input.getAttribute('aria-describedby')}`);
      expect(text(hint)).toBe('Lettres, chiffres, - et _ uniquement ; il fera partie du chemin de ses dépôts.');
    });

    it('cannot be submitted without a name', async () => {
      const { fixture } = await openDialog();
      typeName(fixture, '   ');
      expect(submitButton().disabled).toBe(true);
    });

    it('creates the subgroup, closes and reloads the group', async () => {
      const { fixture, http } = await openDialog();
      typeName(fixture, 'infra');
      submitButton().click();
      fixture.detectChanges();

      const req = http.expectOne('/api/groups/group-1/subgroups');
      expect(req.request.body).toEqual({ name: 'infra', description: '' });
      req.flush({ id: 'child-2', parentGroupId: 'group-1', name: 'infra', description: '', createdAt: '2026-02-01T00:00:00Z' });
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      http.expectOne('/api/groups/group-1/children').flush([CHILD]);
      http.expectOne('/api/groups/group-1/repositories').flush([REPO]);
    });

    it('keeps the dialog and its draft open with the reason when the creation fails', async () => {
      const { fixture, http } = await openDialog();
      typeName(fixture, 'bad name');
      submitButton().click();
      fixture.detectChanges();
      http.expectOne('/api/groups/group-1/subgroups').flush({ error: 'invalid' }, { status: 400, statusText: 'Bad Request' });
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(dialog()).not.toBeNull();
      expect(dialog()!.querySelector<HTMLInputElement>('.group-detail__create-name input')!.value).toBe('bad name');
      expect(text(dialog()!.querySelector('gbt-alert'))).toContain('Nom invalide');
    });

    it('ignores Escape, the backdrop, the close button and "Annuler" while the creation runs, and closes again once it failed', async () => {
      const { fixture, http } = await openDialog();
      typeName(fixture, 'infra');
      submitButton().click();
      fixture.detectChanges();

      const cancel = Array.from(dialog()!.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Annuler')!;
      expect(text(dialog()!.querySelector('[role="status"]'))).toBe('Création en cours');
      expect(cancel.disabled).toBe(true);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      document.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      dialog()!.querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      cancel.click();
      fixture.detectChanges();
      expect(dialog()).not.toBeNull();
      expect(dialog()!.querySelector<HTMLInputElement>('.group-detail__create-name input')!.value).toBe('infra');

      http.expectOne('/api/groups/group-1/subgroups').flush({ error: 'invalid' }, { status: 400, statusText: 'Bad Request' });
      fixture.detectChanges();
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      fixture.detectChanges();
      expect(dialog()).toBeNull();
    });

    it('starts empty again after being closed', async () => {
      const { fixture, el } = await openDialog();
      typeName(fixture, 'brouillon');
      Array.from(dialog()!.querySelectorAll<HTMLButtonElement>('button'))
        .find((b) => text(b) === 'Annuler')!
        .click();
      fixture.detectChanges();
      expect(dialog()).toBeNull();

      headerButton(el, 'Nouveau sous-groupe')!.click();
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();
      expect(dialog()!.querySelector<HTMLInputElement>('.group-detail__create-name input')!.value).toBe('');
    });
  });

  describe('aside', () => {
    const panel = (el: HTMLElement, heading: string) =>
      Array.from(el.querySelectorAll<HTMLElement>('[page-aside] gbt-panel')).find((p) => text(p.querySelector('h2')) === heading);

    it("tells the group's path and the caller's role in À propos", () => {
      const { el } = loaded('maintainer');
      const about = panel(el, 'À propos')!;
      expect(text(about)).toContain('acme/backend');
      expect(text(about)).toContain('Mainteneur');
    });

    it('lays the facts out as an inline description list, values on the end edge: the path in a code chip, the role in a badge', () => {
      const { el } = loaded('maintainer');
      const facts = panel(el, 'À propos')!.querySelector('gbt-description-list dl')!;
      expect(facts.getAttribute('data-layout')).toBe('inline');
      expect(facts.getAttribute('data-value-align')).toBe('end');
      expect(Array.from(facts.querySelectorAll('dt'), (dt) => text(dt))).toEqual(['Chemin', 'Votre rôle']);
      const values = facts.querySelectorAll('dd');
      expect(text(values[0].querySelector('code.group-detail__path'))).toBe('acme/backend');
      expect(values[0].querySelector('code')?.getAttribute('title')).toBe('acme/backend');
      expect(text(values[1].querySelector('gbt-badge'))).toBe('Mainteneur');
    });

    it('lists only the path for someone without a role', () => {
      const { el } = loaded(null);
      const facts = panel(el, 'À propos')!.querySelector('gbt-description-list dl')!;
      expect(Array.from(facts.querySelectorAll('dt'), (dt) => text(dt))).toEqual(['Chemin']);
    });

    it('lists the first members as chips with a link to all of them', () => {
      const { el } = loaded('reader');
      const members = panel(el, 'Membres')!;
      const names = Array.from(members.querySelectorAll('gbt-user-chip .gbt-user-chip__name'), (name) => text(name));
      expect(names).toEqual(['alice', 'bob', 'carol', 'dan', 'eve']);
      const all = members.querySelector<HTMLAnchorElement>('.group-detail__all-members')!;
      expect(all.getAttribute('href')).toBe('/groups/group-1/members');
      expect(text(all)).toBe('Voir les 6 membres');
    });

    it('offers to manage a lone member to a maintainer, and only to view them to a reader', () => {
      const one = [MEMBERS[0]];
      const maintainer = loaded('maintainer', { members: one });
      expect(text(maintainer.el.querySelector('.group-detail__all-members'))).toBe('Gérer les membres');
    });

    it('does not tell a reader to "Gérer les membres" when there is one member or fewer', () => {
      const reader = loaded('reader', { members: [MEMBERS[0]] });
      expect(text(reader.el.querySelector('.group-detail__all-members'))).toBe('Voir les membres');
    });

    it('says so quietly when the members cannot be loaded', () => {
      const ctx = setup('group-1', ['acme', 'backend'], 'reader');
      ctx.fixture.detectChanges();
      ctx.http.expectOne('/api/groups/group-1/children').flush([]);
      ctx.http.expectOne('/api/groups/group-1/repositories').flush([]);
      ctx.http.expectOne('/api/groups/group-1/members').flush({}, { status: 500, statusText: 'Internal Server Error' });
      ctx.fixture.detectChanges();

      expect(text(panel(ctx.el, 'Membres'))).toContain("n'ont pas pu être chargés");
      const failed = panel(ctx.el, 'Membres')!.querySelector('gbt-alert .gbt-alert');
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBe('alert');
      expect(failed?.getAttribute('aria-live')).toBeNull();
    });

    it('does not ask for the members of a group the caller has no role in', () => {
      const { el } = loaded(null);
      expect(panel(el, 'Membres')).toBeUndefined();
    });
  });
});
