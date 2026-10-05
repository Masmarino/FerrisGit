import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { Observable, of, Subject, throwError } from 'rxjs';
import { Avatar } from '@masmarino/gabarit/avatar';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { GroupMembers } from './group-members';
import { GroupMember, GroupMembership, GroupsService } from '../groups.service';

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

const MEMBERS: GroupMember[] = [
  { userId: 'u1', username: 'alice', role: 'maintainer', createdAt: '2026-01-01T00:00:00Z' },
  { userId: 'u2', username: 'bob', role: 'reader', createdAt: '2026-02-01T00:00:00Z' },
];

describe('GroupMembers', () => {
  function setup(options: { role?: GroupMembership['role'] | null; members?: () => Observable<GroupMember[]>; memberships?: () => Observable<GroupMembership[]> } = {}) {
    const memberships: GroupMembership[] = options.role === null ? [] : [{ id: 'group-1', path: 'acme/backend', role: options.role ?? 'maintainer' }];
    const groupsStub = {
      listMembers: vi.fn(options.members ?? (() => of(MEMBERS))),
      listMember: vi.fn(options.memberships ?? (() => of(memberships))),
      addMember: vi.fn(() => of<void>(undefined)),
      setMemberRole: vi.fn((): Observable<void> => of(undefined)),
      removeMember: vi.fn((): Observable<void> => of(undefined)),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: ActivatedRoute, useValue: { snapshot: { paramMap: convertToParamMap({ id: 'group-1' }) } } },
        { provide: GroupsService, useValue: groupsStub },
        { provide: GbtToastService, useValue: toastStub },
        { provide: LOCALE_ID, useValue: 'fr' },
      ],
    });
    const fixture = TestBed.createComponent(GroupMembers);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, groupsStub, toastStub };
  }

  const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.group-members__rows > li'));
  const button = (root: Element, label: string) =>
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label || b.getAttribute('aria-label') === label);
  const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]');

  it('shows the members card with its icon and an avatar per member', () => {
    const { fixture } = setup();

    const card = fixture.debugElement.queryAll(By.css('gbt-card')).find((c) => text(c.nativeElement.querySelector('h2')).startsWith('Membres'))!;
    expect(card).toBeTruthy();
    expect((card.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name()).toBe('user');
    const avatar = fixture.debugElement.query(By.css('ul li'))?.query(By.directive(Avatar));
    expect((avatar?.componentInstance as Avatar | undefined)?.name()).toBe('alice');
  });

  it('titles the page and links back to the group', () => {
    const { el } = setup();
    expect(Array.from(el.querySelectorAll('h1'), (h) => text(h))).toEqual(['Membres du groupe']);
    const back = el.querySelector<HTMLAnchorElement>('.group-members__back')!;
    expect(back.getAttribute('href')).toBe('/repositories/acme/backend');
    expect(text(back)).toContain('acme/backend');
  });

  it('names each role select after its member, with a label that stays hidden (the row already says who)', () => {
    const { el } = setup();
    const [alice] = rows(el);
    const label = alice.querySelector('gbt-select label')!;

    expect(text(label)).toBe('Rôle de alice');
    expect(label.classList).toContain('gbt-select__label--hidden');
  });

  it('lists each member with their chip and when they joined', () => {
    const { el } = setup();
    const [alice, bob] = rows(el);
    expect(text(alice.querySelector('gbt-user-chip .gbt-user-chip__name'))).toBe('alice');
    expect(alice.querySelector('.gbt-list-row__meta time')?.getAttribute('datetime')).toBe('2026-01-01T00:00:00Z');
    expect(text(bob.querySelector('.gbt-list-row__meta'))).toContain('membre depuis');
  });

  it('explains the roles in the aside', () => {
    const { el } = setup();
    const legend = el.querySelector('[page-aside] gbt-panel')!;
    expect(text(legend.querySelector('h2'))).toBe('Rôles');
    expect(text(legend)).toContain('Lecteur');
    expect(text(legend)).toContain('Contributeur');
    expect(text(legend)).toContain('Mainteneur');
    const terms = Array.from(legend.querySelectorAll('dt')).map((term) => [text(term), text(term.nextElementSibling)]);
    expect(terms).toEqual([
      ['Lecteur', 'Consulte le groupe et ses dépôts.'],
      ['Contributeur', 'Pousse des branches, ouvre des tickets et des demandes de fusion.'],
      ['Mainteneur', 'Administre aussi le groupe : membres, sous-groupes et dépôts.'],
    ]);
  });

  describe('for a maintainer', () => {
    it('adds a member with the chosen role, clears the field and reloads the list', async () => {
      const { fixture, el, groupsStub, toastStub } = setup();
      await fixture.whenStable(); // NgForm registers its ngModel controls a microtask later
      fixture.detectChanges();
      const input = el.querySelector<HTMLInputElement>('.group-members__add input')!;
      input.value = 'carol';
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();
      button(el.querySelector('.group-members__add')!, 'Ajouter le membre')!.click();
      fixture.detectChanges();

      expect(groupsStub.addMember).toHaveBeenCalledWith('group-1', 'carol', 'contributor');
      expect(toastStub.show).toHaveBeenCalledWith('Membre ajouté.');
      expect(groupsStub.listMembers).toHaveBeenCalledTimes(2);
      expect(fixture.componentInstance['newMemberUsername']()).toBe('');
    });

    it('does not add a member with a blank username', () => {
      const { fixture, el, groupsStub } = setup();
      fixture.componentInstance['newMemberUsername'].set('   ');
      fixture.detectChanges();
      expect(button(el.querySelector('.group-members__add')!, 'Ajouter le membre')!.disabled).toBe(true);
      fixture.componentInstance.addMember();
      expect(groupsStub.addMember).not.toHaveBeenCalled();
    });

    it('removes a member after a light confirmation', () => {
      const { fixture, el, groupsStub, toastStub } = setup();
      button(rows(el)[1], 'Retirer bob')!.click();
      fixture.detectChanges();
      expect(text(dialog()!.querySelector('h2'))).toBe('Retirer le membre');

      button(dialog()!, 'Retirer')!.click();
      fixture.detectChanges();
      expect(groupsStub.removeMember).toHaveBeenCalledWith('group-1', 'bob');
      expect(dialog()).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith('Membre retiré.');
    });

    it('does nothing when the removal is cancelled', () => {
      const { fixture, el, groupsStub } = setup();
      button(rows(el)[1], 'Retirer bob')!.click();
      fixture.detectChanges();
      button(dialog()!, 'Annuler')!.click();
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      expect(groupsStub.removeMember).not.toHaveBeenCalled();
    });

    describe('changing a role in place', () => {
      const trigger = (row: Element) => row.querySelector<HTMLButtonElement>('gbt-select .gbt-select__trigger')!;
      async function settle(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }) {
        fixture.detectChanges();
        await fixture.whenStable(); // ngModel writes the select's value asynchronously
        fixture.detectChanges();
      }
      async function pick(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }, row: Element, label: string) {
        trigger(row).click();
        fixture.detectChanges();
        Array.from(row.querySelectorAll<HTMLButtonElement>('[role="option"]'))
          .find((option) => text(option) === label)!
          .click();
        await settle(fixture);
      }

      it('saves the picked role without reloading the list', async () => {
        const { fixture, el, groupsStub, toastStub } = setup();
        await settle(fixture);
        const bob = rows(el)[1];

        await pick(fixture, bob, 'Contributeur');

        expect(groupsStub.setMemberRole).toHaveBeenCalledExactlyOnceWith('group-1', 'bob', 'contributor');
        expect(text(trigger(bob))).toContain('Contributeur');
        expect(toastStub.show).toHaveBeenCalledWith('Rôle mis à jour.');
        expect(groupsStub.listMembers).toHaveBeenCalledTimes(1);
      });

      it('shows the previous role again when the change is refused, and the next pick sends the right role', async () => {
        const { fixture, el, groupsStub, toastStub } = setup();
        await settle(fixture);
        const bob = rows(el)[1];
        const refused = new Subject<void>();
        groupsStub.setMemberRole.mockReturnValue(refused);

        await pick(fixture, bob, 'Mainteneur');
        expect(text(trigger(bob))).toContain('Mainteneur');

        refused.error({ status: 403 });
        await settle(fixture);

        expect(toastStub.show).toHaveBeenCalledWith("Impossible de modifier le rôle de ce membre (vous n'êtes peut-être pas mainteneur de ce groupe).", 'error');
        expect(text(trigger(bob))).toContain('Lecteur');

        groupsStub.setMemberRole.mockReturnValue(of(undefined));
        await pick(fixture, bob, 'Contributeur');
        expect(groupsStub.setMemberRole).toHaveBeenLastCalledWith('group-1', 'bob', 'contributor');
        expect(text(trigger(bob))).toContain('Contributeur');
      });
    });
  });

  describe('for a reader', () => {
    it('shows the members read-only: no add card, the role as text, no remove button', () => {
      const { el } = setup({ role: 'reader' });
      expect(el.querySelector('.group-members__add')).toBeNull();
      const [alice] = rows(el);
      expect(alice.querySelector('gbt-select')).toBeNull();
      expect(text(alice.querySelector('.group-members__role'))).toBe('Mainteneur');
      expect(button(alice, 'Retirer alice')).toBeUndefined();
    });
  });

  describe('while the caller\'s role is being resolved', () => {
    it('holds the add card and the row controls back until the role is known, then shows them to a maintainer', () => {
      const membership$ = new Subject<GroupMembership[]>();
      const { fixture, el } = setup({ memberships: () => membership$ });

      expect(fixture.componentInstance['membershipState']()).toBe('loading');
      expect(el.querySelector('.group-members__add')).toBeNull();
      expect(rows(el).length).toBe(2);
      expect(el.querySelector('.group-members__rows gbt-select')).toBeNull();
      expect(el.querySelector('.group-members__rows .group-members__remove')).toBeNull();
      expect(el.querySelector('.group-members__rows .group-members__role')).toBeNull();

      membership$.next([{ id: 'group-1', path: 'acme/backend', role: 'maintainer' }]);
      membership$.complete();
      fixture.detectChanges();

      expect(fixture.componentInstance['membershipState']()).toBe('known');
      expect(el.querySelector('.group-members__add')).not.toBeNull();
      expect(el.querySelectorAll('.group-members__rows gbt-select').length).toBe(2);
    });

    it('settles on the read-only list once the caller turns out to be a reader', () => {
      const membership$ = new Subject<GroupMembership[]>();
      const { fixture, el } = setup({ memberships: () => membership$ });

      membership$.next([{ id: 'group-1', path: 'acme/backend', role: 'reader' }]);
      membership$.complete();
      fixture.detectChanges();

      expect(el.querySelector('.group-members__add')).toBeNull();
      expect(el.querySelector('.group-members__rows gbt-select')).toBeNull();
      expect(text(rows(el)[0].querySelector('.group-members__role'))).toBe('Mainteneur');
    });

    it('says so and shows the controls anyway when the memberships cannot be loaded (the server enforces the role)', () => {
      const membership$ = new Subject<GroupMembership[]>();
      const { fixture, el, toastStub } = setup({ memberships: () => membership$ });
      expect(toastStub.show).not.toHaveBeenCalled();

      membership$.error(new Error('500'));
      fixture.detectChanges();

      expect(fixture.componentInstance['membershipState']()).toBe('failed');
      expect(toastStub.show).toHaveBeenCalledTimes(1);
      expect(toastStub.show).toHaveBeenCalledWith(expect.stringContaining('rôle'), 'error');
      expect(el.querySelector('.group-members__add')).not.toBeNull();
      expect(el.querySelectorAll('.group-members__rows gbt-select').length).toBe(2);
      expect(button(rows(el)[1], 'Retirer bob')).toBeDefined();
    });
  });

  it('falls back to the groups list for the back link when the group is not among the memberships', () => {
    const { el } = setup({ role: null });
    const back = el.querySelector<HTMLAnchorElement>('.group-members__back')!;
    expect(back.getAttribute('href')).toBe('/repositories?tab=groups');
  });

  it('shows skeleton rows while loading, then explains an empty list', () => {
    const pending = new Subject<GroupMember[]>();
    const { fixture, el } = setup({ members: () => pending });
    expect(el.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(3);
      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des membres…');
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();

    pending.next([]);
    fixture.detectChanges();
    expect(el.querySelector('gbt-skeleton-list')).toBeNull();
    expect(text(el.querySelector('gbt-empty-state'))).toContain("Aucun membre pour l'instant");
  });

  it('shows a failed state with a retry button when the list cannot be loaded', () => {
    let fail = true;
    const { fixture, el, toastStub } = setup({ members: () => (fail ? throwError(() => ({ status: 500 })) : of(MEMBERS)) });
    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les membres. Réessayez plus tard.', 'error');
    const failed = el.querySelector('gbt-alert .gbt-alert');
    expect(text(failed)).toContain("Les membres n'ont pas pu être chargés");
    expect(failed?.getAttribute('data-variant')).toBe('error');
    expect(failed?.getAttribute('role')).toBeNull();
    expect(failed?.getAttribute('aria-live')).toBeNull();
    expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
    expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);

    fail = false;
    button(el, 'Réessayer')!.click();
    fixture.detectChanges();
    expect(rows(el)).toHaveLength(2);
  });
});
