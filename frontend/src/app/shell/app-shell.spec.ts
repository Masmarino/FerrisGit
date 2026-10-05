import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController } from '@angular/common/http/testing';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter, Routes } from '@angular/router';
import { AppShell } from './app-shell';
import { RepositoryContextService } from '../repositories/repository-context.service';
import { PageTitleService } from './page-title.service';
import { MeService } from './me.service';
import { SidebarCollapseService } from './sidebar-collapse.service';
import { AuthService } from '../auth/auth.service';
import { BreadcrumbSwitcherService } from './breadcrumb-switcher.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';

// Catch-all target so navigateByUrl ends in a NavigationEnd; routerLinkActive only updates after a successful navigation.
@Component({ selector: 'fg-test-empty', template: '', standalone: true })
class EmptyTestComponent {}

describe('AppShell', () => {
  function setup(routes: Routes = []) {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter(routes)] });
    const fixture = TestBed.createComponent(AppShell);
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance };
  }

  type Fixture = ReturnType<typeof setup>['fixture'];

  function userMenuTrigger(fixture: Fixture): HTMLButtonElement {
    return fixture.nativeElement.querySelector('.app-shell__header-end button[aria-haspopup="menu"]');
  }

  function userMenu(fixture: Fixture): HTMLElement | null {
    return fixture.nativeElement.querySelector('.app-shell__header-end [role="menu"]');
  }

  function isMenuOpen(fixture: Fixture): boolean {
    return userMenu(fixture) !== null;
  }

  function userMenuItems(fixture: Fixture): HTMLElement[] {
    return Array.from(fixture.nativeElement.querySelectorAll('.app-shell__header-end [role="menuitem"]'));
  }

  async function openUserMenu(fixture: Fixture): Promise<void> {
    userMenuTrigger(fixture).click();
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  async function pressOnMenu(fixture: Fixture, key: string): Promise<void> {
    (document.activeElement as HTMLElement).dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  it('shows the running version at the foot of the navigation', () => {
    const { fixture } = setup();
    TestBed.inject(HttpTestingController).expectOne('/api/version').flush({ version: '0.1.3' });
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.gbt-app-shell__version')?.textContent?.trim()).toBe('v0.1.3');
  });

  describe('user menu', () => {
    it('shows the username on the trigger and keeps the menu closed until it is used', () => {
      const { fixture } = setup();
      TestBed.inject(MeService).username.set('julien');
      fixture.detectChanges();

      const trigger = userMenuTrigger(fixture);
      expect(trigger.textContent?.trim()).toBe('julien');
      expect(trigger.getAttribute('aria-expanded')).toBe('false');
      expect(isMenuOpen(fixture)).toBe(false);
    });

    it('names the trigger "Compte" until /api/auth/me has answered with the username', () => {
      const { fixture } = setup();

      const trigger = userMenuTrigger(fixture);
      expect(trigger.textContent?.trim()).toBe('Compte');

      TestBed.inject(MeService).username.set('julien');
      fixture.detectChanges();
      expect(trigger.textContent?.trim()).toBe('julien');
    });

    it('opens on click with "Mon compte", "Documentation" and "Déconnexion" and closes on a second click', () => {
      const { fixture } = setup();
      const trigger = userMenuTrigger(fixture);

      trigger.click();
      fixture.detectChanges();
      expect(isMenuOpen(fixture)).toBe(true);
      expect(trigger.getAttribute('aria-expanded')).toBe('true');
      expect(userMenuItems(fixture).map((item) => item.textContent?.trim())).toEqual(['Mon compte', 'Documentation', 'Déconnexion']);

      trigger.click();
      fixture.detectChanges();
      expect(isMenuOpen(fixture)).toBe(false);
      expect(trigger.getAttribute('aria-expanded')).toBe('false');
    });

    it('writes its items with Gabarit\'s menu item: two links and a button, each with a decorative icon', () => {
      const { fixture } = setup();
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      const [account, docs, logout] = userMenuItems(fixture);
      expect(account.tagName).toBe('A');
      expect(docs.tagName).toBe('A');
      expect(logout.tagName).toBe('BUTTON');
      expect(logout.getAttribute('type')).toBe('button');
      expect(userMenuItems(fixture).map((item) => item.getAttribute('tabindex'))).toEqual(['-1', '-1', '-1']);
      expect(userMenuItems(fixture).map((item) => item.querySelector('gbt-icon.gbt-menu-item__icon')?.getAttribute('aria-hidden'))).toEqual(['true', 'true', 'true']);
    });

    it('gives the focus back to the menu button once "Mon compte" has been chosen', async () => {
      const { fixture } = setup([{ path: 'account', component: EmptyTestComponent }]);
      await openUserMenu(fixture);
      expect(userMenuItems(fixture)[0]).toBe(document.activeElement);

      userMenuItems(fixture)[0].click();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(isMenuOpen(fixture)).toBe(false);
      expect(document.activeElement).toBe(userMenuTrigger(fixture));
    });

    it('links "Mon compte" to /account', () => {
      const { fixture } = setup();
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      const account = userMenuItems(fixture)[0];
      expect(account.getAttribute('href')).toBe('/account');
    });

    it('links "Documentation" to /docs', () => {
      const { fixture } = setup();
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      expect(userMenuItems(fixture)[1].getAttribute('href')).toBe('/docs');
    });

    it('closes after choosing "Mon compte", which navigates to /account', async () => {
      const { fixture } = setup([{ path: 'account', component: EmptyTestComponent }]);
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      userMenuItems(fixture)[0].click();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(isMenuOpen(fixture)).toBe(false);
      expect(TestBed.inject(Router).url).toBe('/account');
    });

    it('logs out and goes to the login page on "Déconnexion"', () => {
      const { fixture } = setup();
      const logout = vi.spyOn(TestBed.inject(AuthService), 'logout').mockImplementation(() => {});
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockResolvedValue(true);
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      userMenuItems(fixture)[2].click();
      fixture.detectChanges();

      expect(logout).toHaveBeenCalledTimes(1);
      expect(navigate).toHaveBeenCalledWith('/login');
      expect(isMenuOpen(fixture)).toBe(false);
    });

    it('closes when clicking outside the component', () => {
      const { fixture } = setup();
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      fixture.detectChanges();

      expect(isMenuOpen(fixture)).toBe(false);
    });

    it('does not close when clicking inside the menu component, outside its item list', () => {
      const { fixture } = setup();
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      const menu = userMenuTrigger(fixture).closest('gbt-menu') as HTMLElement;
      menu.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      fixture.detectChanges();

      expect(isMenuOpen(fixture)).toBe(true);
    });

    it('closes when clicking elsewhere in the app shell (e.g. the nav)', () => {
      const { fixture } = setup();
      userMenuTrigger(fixture).click();
      fixture.detectChanges();

      const nav = fixture.nativeElement.querySelector('.app-shell__nav-list') as HTMLElement;
      nav.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      fixture.detectChanges();

      expect(isMenuOpen(fixture)).toBe(false);
    });

    it('moves focus to the first item when opened with a click', async () => {
      const { fixture } = setup();

      await openUserMenu(fixture);

      expect(document.activeElement).toBe(userMenuItems(fixture)[0]);
    });

    it('moves focus through the items with ArrowDown/ArrowUp, wrapping at both ends', async () => {
      const { fixture } = setup();
      await openUserMenu(fixture);
      const [account, docs, logout] = userMenuItems(fixture);

      await pressOnMenu(fixture, 'ArrowDown');
      expect(document.activeElement).toBe(docs);
      await pressOnMenu(fixture, 'ArrowDown');
      expect(document.activeElement).toBe(logout);
      await pressOnMenu(fixture, 'ArrowDown');
      expect(document.activeElement).toBe(account);
      await pressOnMenu(fixture, 'ArrowUp');
      expect(document.activeElement).toBe(logout);
    });

    it('jumps to the first and last item with Home and End', async () => {
      const { fixture } = setup();
      await openUserMenu(fixture);
      const [account, , logout] = userMenuItems(fixture);

      await pressOnMenu(fixture, 'End');
      expect(document.activeElement).toBe(logout);
      await pressOnMenu(fixture, 'Home');
      expect(document.activeElement).toBe(account);
    });

    it('opens with ArrowDown on the closed trigger and focuses the first item', async () => {
      const { fixture } = setup();
      const trigger = userMenuTrigger(fixture);
      trigger.focus();

      await pressOnMenu(fixture, 'ArrowDown');

      expect(isMenuOpen(fixture)).toBe(true);
      expect(document.activeElement).toBe(userMenuItems(fixture)[0]);
    });

    it('opens with ArrowUp on the closed trigger and focuses the last item', async () => {
      const { fixture } = setup();
      userMenuTrigger(fixture).focus();

      await pressOnMenu(fixture, 'ArrowUp');

      expect(isMenuOpen(fixture)).toBe(true);
      expect(document.activeElement).toBe(userMenuItems(fixture)[2]);
    });

    it('closes on Escape and returns focus to the trigger', async () => {
      const { fixture } = setup();
      await openUserMenu(fixture);
      expect(document.activeElement).toBe(userMenuItems(fixture)[0]);

      await pressOnMenu(fixture, 'Escape');

      expect(isMenuOpen(fixture)).toBe(false);
      expect(document.activeElement).toBe(userMenuTrigger(fixture));
    });
  });

  it('renders the mobile nav toggle from gbt-app-shell', () => {
    const { fixture } = setup();

    const toggle: HTMLButtonElement = fixture.nativeElement.querySelector('.gbt-app-shell__toggle');
    expect(toggle).toBeTruthy();
    expect(toggle.getAttribute('aria-label')).toBe('Ouvrir le menu de navigation');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
  });

  describe('quick search', () => {
    const settle = async (fixture: Fixture): Promise<void> => {
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();
    };

    it('sits in the header as a button that names itself and its shortcut', () => {
      const { fixture } = setup();

      const trigger: HTMLButtonElement = fixture.nativeElement.querySelector('.app-shell__header gbt-command-palette-trigger button');
      expect(trigger.getAttribute('aria-label')).toBe('Rechercher ou aller à…');
      expect(trigger.getAttribute('aria-keyshortcuts')).toMatch(/^(Meta|Control)\+K$/);
    });

    it('signs out from the palette as from the account menu', async () => {
      const { fixture } = setup();
      const logout = vi.spyOn(TestBed.inject(AuthService), 'logout').mockImplementation(() => {});
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockResolvedValue(true);
      fixture.nativeElement.querySelector('.app-shell__header gbt-command-palette-trigger button').click();
      await settle(fixture);

      // Outside the header, whose dark theme it would otherwise take on.
      expect(fixture.nativeElement.querySelector('fg-quick-search [role="dialog"]')).not.toBeNull();
      expect(fixture.nativeElement.querySelector('.app-shell__header [role="dialog"]')).toBeNull();
      const field = document.querySelector<HTMLInputElement>('[role="dialog"] input[role="combobox"]')!;
      field.value = 'déconnexion';
      field.dispatchEvent(new Event('input'));
      await settle(fixture);
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      await settle(fixture);

      expect(logout).toHaveBeenCalledTimes(1);
      expect(navigate).toHaveBeenCalledWith('/login');
    });
  });

  it('renders a skip-to-content link pointing at the main content landmark', () => {
    const { fixture } = setup();

    const skipLink: HTMLAnchorElement = fixture.nativeElement.querySelector('.gbt-app-shell__skip');
    expect(skipLink.textContent?.trim()).toBe('Aller au contenu principal');

    const targetId = skipLink.getAttribute('href')!.replace('#', '');
    const main = fixture.nativeElement.querySelector(`#${targetId}`);
    expect(main).toBeTruthy();
    expect(main.classList.contains('gbt-app-shell__content')).toBe(true);
  });

  describe('toasts', () => {
    const items = (fixture: Fixture): HTMLElement[] => Array.from(fixture.nativeElement.querySelectorAll('gbt-toaster .gbt-toaster__item'));

    it('shows the toasts of the service, warnings and errors as alerts and the others as polite statuses', () => {
      const { fixture } = setup();
      const toasts = TestBed.inject(GbtToastService);

      toasts.show('Dépôt créé.');
      toasts.show('Le jeton expire bientôt.', 'warning');
      toasts.show('Fusion impossible.', 'error');
      toasts.show('Pipeline lancé.', 'info');
      fixture.detectChanges();

      expect(items(fixture).map((item) => [item.getAttribute('data-variant'), item.getAttribute('role'), item.querySelector('.gbt-toaster__message')?.textContent?.trim()])).toEqual([
        ['success', 'status', 'Dépôt créé.'],
        ['warning', 'alert', 'Le jeton expire bientôt.'],
        ['error', 'alert', 'Fusion impossible.'],
        ['info', 'status', 'Pipeline lancé.'],
      ]);
    });

    it('names each close button in French and removes the toast on it', () => {
      const { fixture } = setup();
      const toasts = TestBed.inject(GbtToastService);
      toasts.show('Dépôt créé.');
      toasts.show('Ticket fermé.');
      fixture.detectChanges();

      const closes = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('gbt-toaster .gbt-toaster__close'));
      expect(closes.map((button) => button.getAttribute('aria-label'))).toEqual(['Fermer', 'Fermer']);
      closes[0].click();
      fixture.detectChanges();

      expect(items(fixture).map((item) => item.querySelector('.gbt-toaster__message')?.textContent?.trim())).toEqual(['Ticket fermé.']);
      expect(toasts.toasts().map((toast) => toast.message)).toEqual(['Ticket fermé.']);
    });

    it('removes a toast after 5 seconds, errors included', () => {
      vi.useFakeTimers();
      try {
        const { fixture } = setup();
        const toasts = TestBed.inject(GbtToastService);
        toasts.show('Dépôt créé.');
        toasts.show('Fusion impossible.', 'error');
        fixture.detectChanges();
        expect(items(fixture)).toHaveLength(2);

        vi.advanceTimersByTime(4999);
        fixture.detectChanges();
        expect(items(fixture)).toHaveLength(2);

        vi.advanceTimersByTime(1);
        fixture.detectChanges();
        expect(items(fixture)).toHaveLength(0);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('repository context sidebar', () => {
    it('renders the global nav when no repository context is active', () => {
      const { fixture } = setup();

      expect(fixture.nativeElement.textContent).toContain('Dépôts');
    });

    it('renders the repository sidebar when a repository context is active', () => {
      const { fixture } = setup();
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: 'reader', ancestors: [], groupId: null });
      fixture.detectChanges();

      // The global items give way to the repository's: the only way out is the link back to the repositories, and the
      // rail names the repository, its owner's path above its name.
      const nav = fixture.nativeElement.querySelector('.app-shell__nav-list') as HTMLElement;
      expect(nav.textContent).not.toContain('Accueil');
      const back = Array.from(nav.querySelectorAll('a')).filter((a) => a.getAttribute('href') === '/repositories');
      expect(back.map((a) => a.textContent?.trim())).toEqual(['Dépôts']);
      expect(nav.querySelector('.app-shell__repo-owner')?.textContent?.trim()).toBe('acme');
      expect(nav.querySelector('.app-shell__repo-name')?.textContent?.trim()).toBe('widget');
      expect(fixture.nativeElement.textContent).toContain('Tickets');
      expect(fixture.nativeElement.textContent).toContain('Demandes de fusion');
      expect(fixture.nativeElement.textContent).not.toContain('Issues');
      expect(fixture.nativeElement.textContent).toContain('Pipelines');
    });

    it('hides Réglages for a reader', () => {
      const { fixture } = setup();
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: 'reader', ancestors: [], groupId: null });
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).not.toContain('Réglages');
    });

    it('shows Réglages for a maintainer', () => {
      const { fixture } = setup();
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: 'maintainer', ancestors: [], groupId: null });
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('Réglages');
    });

    it('hides Réglages when the role has not resolved yet (role: null)', () => {
      const { fixture } = setup();
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: null, ancestors: [], groupId: null });
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).not.toContain('Réglages');
    });

    it('marks only "Aperçu" active via exact routerLinkActiveOptions, while other links stay active on their sub-routes', async () => {
      const { fixture } = setup([{ path: '**', component: EmptyTestComponent }]);
      const router = TestBed.inject(Router);
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: 'owner', ancestors: [], groupId: null });
      fixture.detectChanges();

      await router.navigateByUrl('/repositories/acme/widget/-/issues/5');
      fixture.detectChanges();

      const links: HTMLAnchorElement[] = Array.from(fixture.nativeElement.querySelectorAll('.app-shell__nav-list a'));
      const issuesLink = links.find((a) => a.textContent?.includes('Tickets'));
      const apercuLink = links.find((a) => a.textContent?.includes('Aperçu'));

      expect(issuesLink).toBeTruthy();
      expect(apercuLink).toBeTruthy();
      expect(issuesLink!.getAttribute('aria-current')).toBe('page');
      expect(apercuLink!.hasAttribute('aria-current')).toBe(false);
    });

    it('builds correct hrefs for each repository nav link', () => {
      const { fixture } = setup();
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: 'owner', ancestors: [], groupId: null });
      fixture.detectChanges();

      const links: HTMLAnchorElement[] = Array.from(fixture.nativeElement.querySelectorAll('.app-shell__nav-list a'));
      const hrefs = links.map((a) => a.getAttribute('href'));
      expect(hrefs).toContain('/repositories/acme/widget');
      expect(hrefs).toContain('/repositories/acme/widget/-/issues');
      expect(hrefs).toContain('/repositories/acme/widget/-/pipelines');
      expect(hrefs).toContain('/repositories/acme/widget/-/merge-requests');
      expect(hrefs).toContain('/repositories/acme/widget/-/releases');
      expect(hrefs).toContain('/repositories/acme/widget/-/wiki');
      expect(hrefs).toContain('/repositories/acme/widget/-/settings');
      expect(hrefs).toContain('/repositories');
    });
  });

  describe('breadcrumb', () => {
    it("shows what sits above the page from its route's trail, the page itself being named by its own title", async () => {
      const { fixture } = setup([
        {
          path: 'admin/users/:id',
          component: EmptyTestComponent,
          data: { trail: [{ label: 'Administration' }, { label: 'Utilisateurs', link: ['/admin', 'users'] }] },
        },
        { path: 'home', component: EmptyTestComponent },
      ]);
      const router = TestBed.inject(Router);

      await router.navigateByUrl('/admin/users/u1');
      fixture.detectChanges();
      const steps = Array.from(fixture.nativeElement.querySelectorAll('.app-shell__breadcrumb li[breadcrumb-ancestor]')) as HTMLElement[];
      expect(steps.map((li) => li.textContent?.trim())).toEqual(['Administration', 'Utilisateurs']);
      expect(steps[0].querySelector('a')).toBeNull();
      expect(steps[1].querySelector('a')?.getAttribute('href')).toBe('/admin/users');

      await router.navigateByUrl('/home');
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelectorAll('.app-shell__breadcrumb li[breadcrumb-ancestor]')).toHaveLength(0);
    });

    // A control-flow block is only projected into a named slot with a single root element, so each item must be a flat
    // sibling in the <ol>.
    it('shows only the page title when no repository context is active', () => {
      const { fixture } = setup();
      TestBed.inject(PageTitleService).set('Groupes');
      fixture.detectChanges();

      const list = fixture.nativeElement.querySelector('.gbt-breadcrumb__list')!;
      expect(list.querySelectorAll('li[breadcrumb-ancestor]').length).toBe(0);
      const current = list.querySelector('.gbt-breadcrumb__current');
      expect(current?.textContent).toContain('Groupes');
    });

    it('renders group ancestors, the repo switcher, and the subpage title as flat siblings', () => {
      const { fixture } = setup();
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({
        repositoryId: 'repo-1',
        path: ['acme', 'team', 'widget'],
        role: 'reader',
        ancestors: [
          { label: 'acme', link: ['/groups', 'acme'] },
          { label: 'team', link: ['/groups', 'acme', 'team'] },
        ],
        groupId: 'group-team',
      });
      TestBed.inject(PageTitleService).set('Tickets');
      fixture.detectChanges();

      const list: HTMLOListElement = fixture.nativeElement.querySelector('.gbt-breadcrumb__list');
      const ancestorItems: HTMLLIElement[] = Array.from(list.querySelectorAll('li[breadcrumb-ancestor]'));
      expect(ancestorItems.length).toBe(3);
      expect(ancestorItems[0].textContent).toContain('acme');
      expect(ancestorItems[1].textContent).toContain('team');
      expect(ancestorItems[2].querySelector('button.gbt-menu__trigger')).toBeTruthy();
      for (const li of ancestorItems) {
        expect(li.parentElement).toBe(list);
        expect(li.closest('.gbt-breadcrumb__current')).toBeNull();
      }

      const current = list.querySelector('.gbt-breadcrumb__current');
      expect(current?.parentElement).toBe(list);
      expect(current?.textContent).toContain('Tickets');
      expect(current?.querySelector('li')).toBeNull();
    });

    it('renders the repo switcher itself as the current segment when there is no subpage title', () => {
      const { fixture } = setup();
      const context = TestBed.inject(RepositoryContextService);
      context.current.set({
        repositoryId: 'repo-1',
        path: ['acme', 'widget'],
        role: 'reader',
        ancestors: [{ label: 'acme', link: ['/groups', 'acme'] }],
        groupId: 'group-acme',
      });
      TestBed.inject(PageTitleService).set('');
      fixture.detectChanges();

      const list: HTMLOListElement = fixture.nativeElement.querySelector('.gbt-breadcrumb__list');
      expect(list.querySelectorAll('li[breadcrumb-ancestor]').length).toBe(1);
      const current = list.querySelector('.gbt-breadcrumb__current');
      expect(current?.parentElement).toBe(list);
      expect(current?.querySelector('button.gbt-menu__trigger')).toBeTruthy();
    });
  });

  describe('repository switcher menu', () => {
    // The data lives in a singleton service, so the tests set it directly.
    function openSwitcher() {
      const { fixture } = setup();
      const switcher = TestBed.inject(BreadcrumbSwitcherService);
      vi.spyOn(switcher, 'loadForGroup').mockImplementation(() => {});
      TestBed.inject(RepositoryContextService).current.set({
        repositoryId: 'repo-1',
        path: ['acme', 'team', 'widget'],
        role: 'reader',
        ancestors: [
          { label: 'acme', link: ['/groups', 'acme'] },
          { label: 'team', link: ['/groups', 'acme', 'team'] },
        ],
        groupId: 'group-team',
      });
      TestBed.inject(PageTitleService).set('Tickets');
      fixture.detectChanges();
      const trigger: HTMLButtonElement = fixture.nativeElement.querySelector('li[breadcrumb-ancestor] button.gbt-menu__trigger');
      const items = (): HTMLElement[] => Array.from(fixture.nativeElement.querySelectorAll('.gbt-breadcrumb__list [role="menuitem"]'));
      return { fixture, switcher, trigger, items };
    }

    it('lists the sibling groups, then the repositories, as links to them', () => {
      const { fixture, switcher, trigger, items } = openSwitcher();
      switcher.groups.set([{ id: 'g1', name: 'core' }]);
      switcher.repositories.set([{ id: 'r1', path: ['acme', 'team', 'gadget'] }]);
      trigger.click();
      fixture.detectChanges();

      expect(items().map((item) => [item.tagName, item.textContent?.trim(), item.getAttribute('href')])).toEqual([
        ['A', 'core', '/groups/acme/team/core'],
        ['A', 'gadget', '/repositories/acme/team/gadget'],
      ]);
      expect(items().every((item) => !item.hasAttribute('aria-disabled'))).toBe(true);
    });

    it('never opens empty: a disabled "Chargement…" item that keyboard focus can still reach, while it loads', async () => {
      const { fixture, switcher, trigger, items } = openSwitcher();
      switcher.loading.set(true);
      trigger.click();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(items().map((item) => item.textContent?.trim())).toEqual(['Chargement…']);
      expect(items()[0].getAttribute('aria-disabled')).toBe('true');
      expect(items()[0].getAttribute('tabindex')).toBe('-1');
      expect(document.activeElement).toBe(items()[0]);
    });

    it('says "Aucun élément" (also disabled) when there is nothing to switch to', () => {
      const { fixture, trigger, items } = openSwitcher();
      trigger.click();
      fixture.detectChanges();

      expect(items().map((item) => item.textContent?.trim())).toEqual(['Aucun élément']);
      expect(items()[0].getAttribute('aria-disabled')).toBe('true');
    });

    it('does not close, nor navigate, when the disabled placeholder is chosen', () => {
      const { fixture, trigger, items } = openSwitcher();
      trigger.click();
      fixture.detectChanges();

      items()[0].click();
      fixture.detectChanges();

      expect(items()).toHaveLength(1);
      expect(trigger.getAttribute('aria-expanded')).toBe('true');
    });
  });

  describe('admin nav group', () => {
    // Folded, the sub links stay in the DOM but `hidden`, so out of the tab order and the accessibility tree.
    const panelHidden = (fixture: Fixture): boolean => (fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__panel') as HTMLElement).hasAttribute('hidden');

    it('omits the Admin entry entirely for a non-admin', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'alice', email: 'a@example.com', isAdmin: false });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();

      const items = fixture.componentInstance.navItems();
      expect(items.some((i) => i.text === 'Administration')).toBe(false);
    });

    it('renders Administration as an expandable group with Tableau de bord/Utilisateurs/Santé/Réglages children for an admin', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'admin', email: 'admin@example.com', isAdmin: true });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();

      const items = fixture.componentInstance.navItems();
      const admin = items.find((i) => i.text === 'Administration');
      expect(admin?.children?.map((c) => c.text)).toEqual(['Tableau de bord', 'Utilisateurs', 'Santé', 'Réglages']);
      expect(admin?.children?.map((c) => c.link)).toEqual(['/admin/dashboard', '/admin/users', '/admin/health', '/admin/settings']);
    });

    it('is one Gabarit nav group: a toggle named "Administration" whose name does not change with its state, and the four sub links in a panel', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'admin', email: 'admin@example.com', isAdmin: true });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();

      const toggle: HTMLButtonElement = fixture.nativeElement.querySelector('gbt-app-shell-nav-group .gbt-app-shell-nav-group__toggle');
      expect(toggle.textContent?.trim()).toBe('Administration');
      expect(toggle.getAttribute('aria-label')).toBeNull();
      expect(toggle.getAttribute('aria-expanded')).toBe('false');
      const panel: HTMLElement = fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__panel');
      expect(toggle.getAttribute('aria-controls')).toBe(panel.id);
      expect(panel.hasAttribute('hidden')).toBe(true);
      const links = Array.from(panel.querySelectorAll<HTMLAnchorElement>('a'));
      expect(links.map((a) => a.textContent?.trim())).toEqual(['Tableau de bord', 'Utilisateurs', 'Santé', 'Réglages']);
      expect(links.map((a) => a.getAttribute('href'))).toEqual(['/admin/dashboard', '/admin/users', '/admin/health', '/admin/settings']);

      toggle.click();
      fixture.detectChanges();

      expect(toggle.textContent?.trim()).toBe('Administration');
      expect(toggle.getAttribute('aria-expanded')).toBe('true');
    });

    it('gives every sub link an icon and a label, so the links survive the icons-only collapsed rail', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'admin', email: 'admin@example.com', isAdmin: true });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();
      (fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__toggle') as HTMLButtonElement).click();
      TestBed.inject(SidebarCollapseService).collapsed.set(true);
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelector('.gbt-app-shell__nav--collapsed gbt-app-shell-nav-group')).toBeTruthy();
      const sublinks = Array.from(fixture.nativeElement.querySelectorAll('.gbt-app-shell-nav-group__panel a') as NodeListOf<HTMLElement>);
      expect(sublinks).toHaveLength(4);
      for (const link of sublinks) {
        expect(link.classList).toContain('gbt-app-shell__link');
        expect(link.querySelector(':scope > gbt-icon')).toBeTruthy();
        expect(link.querySelector(':scope > span')?.textContent?.trim()).toBeTruthy();
      }
      const toggle: HTMLElement = fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__toggle');
      expect(toggle.querySelector('gbt-icon')).toBeTruthy();
    });

    it('expands the Admin group in the DOM on click, and collapses it again on a second click', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'admin', email: 'admin@example.com', isAdmin: true });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();

      const toggle: HTMLButtonElement = fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__toggle');
      expect(toggle).toBeTruthy();
      expect(toggle.getAttribute('aria-expanded')).toBe('false');
      expect(panelHidden(fixture)).toBe(true);

      toggle.click();
      fixture.detectChanges();
      expect(panelHidden(fixture)).toBe(false);
      expect(toggle.getAttribute('aria-expanded')).toBe('true');

      toggle.click();
      fixture.detectChanges();
      expect(toggle.getAttribute('aria-expanded')).toBe('false');
      expect(panelHidden(fixture)).toBe(true);
    });

    it('auto-expands the Admin group on load when the current URL is one of its children (e.g. reloading on /admin/health)', async () => {
      // Navigate before creating the component, like a reload or a deep link: the router already knows the URL when the
      // constructor first reads it.
      TestBed.configureTestingModule({
        providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: '**', component: EmptyTestComponent }])],
      });
      const router = TestBed.inject(Router);
      await router.navigateByUrl('/admin/health');

      const fixture = TestBed.createComponent(AppShell);
      fixture.detectChanges();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'admin', email: 'admin@example.com', isAdmin: true });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();

      const toggle: HTMLButtonElement = fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__toggle');
      expect(toggle).toBeTruthy();
      expect(toggle.getAttribute('aria-expanded')).toBe('true');
      expect(panelHidden(fixture)).toBe(false);
    });

    it('still collapses an auto-expanded group on click, and re-expands it on a second click', async () => {
      TestBed.configureTestingModule({
        providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: '**', component: EmptyTestComponent }])],
      });
      const router = TestBed.inject(Router);
      await router.navigateByUrl('/admin/health');

      const fixture = TestBed.createComponent(AppShell);
      fixture.detectChanges();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'admin', email: 'admin@example.com', isAdmin: true });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();

      const toggle: HTMLButtonElement = fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__toggle');
      expect(toggle.getAttribute('aria-expanded')).toBe('true');

      toggle.click();
      fixture.detectChanges();
      expect(toggle.getAttribute('aria-expanded')).toBe('false');
      expect(panelHidden(fixture)).toBe(true);

      toggle.click();
      fixture.detectChanges();
      expect(toggle.getAttribute('aria-expanded')).toBe('true');
      expect(panelHidden(fixture)).toBe(false);
    });

    // A manual toggle sticks, whatever navigation does afterwards.
    it('keeps a manual override in effect even after navigating away and back to the group', async () => {
      TestBed.configureTestingModule({
        providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: '**', component: EmptyTestComponent }])],
      });
      const router = TestBed.inject(Router);
      await router.navigateByUrl('/admin/health');

      const fixture = TestBed.createComponent(AppShell);
      fixture.detectChanges();
      const httpMock = TestBed.inject(HttpTestingController);
      httpMock.expectOne('/api/auth/me').flush({ username: 'admin', email: 'admin@example.com', isAdmin: true });
      httpMock.expectOne('/api/settings/public').flush({ executionEngine: 'docker-runners' });
      httpMock.expectOne('/api/notifications/unread-count').flush({ count: 0 });
      fixture.detectChanges();

      const toggle = () => fixture.nativeElement.querySelector('.gbt-app-shell-nav-group__toggle') as HTMLButtonElement;
      expect(toggle().getAttribute('aria-expanded')).toBe('true');

      toggle().click();
      fixture.detectChanges();
      expect(toggle().getAttribute('aria-expanded')).toBe('false');

      await router.navigateByUrl('/home');
      fixture.detectChanges();
      await router.navigateByUrl('/admin/dashboard');
      fixture.detectChanges();
      expect(toggle().getAttribute('aria-expanded')).toBe('false');
    });
  });
});
