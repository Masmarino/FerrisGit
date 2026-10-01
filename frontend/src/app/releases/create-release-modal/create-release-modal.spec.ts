import { TestBed } from '@angular/core/testing';
import { Subject, of, throwError } from 'rxjs';
import { CreateReleaseModal } from './create-release-modal';
import { ReleasesService } from '../releases.service';
import { MergeRequestsService } from '../../merge-requests/merge-requests.service';

describe('CreateReleaseModal', () => {
  function setup() {
    const releasesStub = {
      listTags: vi.fn(() => of([{ name: 'v0.9.0', targetSha: 'deadbeef' }])),
      create: vi.fn(() => of({ id: '1', tagName: 'v1.0.0', title: 'x', draft: true, prerelease: false, authorId: 'a', createdAt: '2026-01-01T00:00:00Z', publishedAt: null })),
      deleteTag: vi.fn(() => of(undefined)),
    };
    const mergeRequestsStub = {
      listBranches: vi.fn(() => of([{ name: 'main', tipSha: 'abc123', isDefault: true }])),
    };
    TestBed.configureTestingModule({ providers: [{ provide: ReleasesService, useValue: releasesStub }, { provide: MergeRequestsService, useValue: mergeRequestsStub }] });
    const fixture = TestBed.createComponent(CreateReleaseModal);
    fixture.componentRef.setInput('repositoryId', 'r1');
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance, releasesStub };
  }

  it('defaults to the "new tag" choice and resolves the target sha from the default branch', () => {
    const { component } = setup();
    expect(component.isNewTag()).toBe(true);
    expect(component.resolvedTargetSha()).toBe('abc123');
  });

  it('resolves the target sha from the chosen existing tag once switched', () => {
    const { component } = setup();
    component.tagChoice.set('v0.9.0');
    expect(component.isNewTag()).toBe(false);
    expect(component.resolvedTargetSha()).toBe('deadbeef');
  });

  it('rejects submission with no title', () => {
    const { component } = setup();
    component.newTagName.set('v1.0.0');
    component.title.set('');
    component.submit();
    expect(component.error()).toBeTruthy();
  });

  it('creates the release with the resolved fields and emits created', () => {
    const { component, releasesStub } = setup();
    component.newTagName.set('v1.0.0');
    component.title.set('First release');
    let created = false;
    component.created.subscribe(() => (created = true));
    component.submit();
    expect(releasesStub.create).toHaveBeenCalledWith('r1', expect.objectContaining({ tagName: 'v1.0.0', targetCommitSha: 'abc123', title: 'First release' }));
    expect(created).toBe(true);
  });

  it('on a 400 while creating a brand-new tag, offers to delete it and retry', () => {
    const { component, releasesStub } = setup();
    releasesStub.create.mockReturnValueOnce(throwError(() => ({ status: 400 })));
    component.newTagName.set('v1.0.0');
    component.title.set('First release');

    component.submit();

    expect(component.conflictedTagName()).toBe('v1.0.0');
    expect(component.error()).toContain('v1.0.0');
  });

  it('does not offer the delete-and-retry option for a non-conflict error', () => {
    const { component, releasesStub } = setup();
    releasesStub.create.mockReturnValueOnce(throwError(() => ({ status: 500 })));
    component.newTagName.set('v1.0.0');
    component.title.set('First release');

    component.submit();

    expect(component.conflictedTagName()).toBeNull();
  });

  it('does not offer delete-and-retry when picking an existing tag, even on a 400', () => {
    const { component, releasesStub } = setup();
    releasesStub.create.mockReturnValueOnce(throwError(() => ({ status: 400 })));
    component.tagChoice.set('v0.9.0');
    component.title.set('First release');

    component.submit();

    expect(component.conflictedTagName()).toBeNull();
  });

  it('clicking delete-and-retry deletes the tag then resubmits, clearing the conflict on success', () => {
    const { component, releasesStub } = setup();
    releasesStub.create.mockReturnValueOnce(throwError(() => ({ status: 400 })));
    component.newTagName.set('v1.0.0');
    component.title.set('First release');
    component.submit();
    expect(component.conflictedTagName()).toBe('v1.0.0');

    let created = false;
    component.created.subscribe(() => (created = true));
    component.deleteConflictedTagAndRetry();

    expect(releasesStub.deleteTag).toHaveBeenCalledWith('r1', 'v1.0.0');
    expect(component.conflictedTagName()).toBeNull();
    expect(created).toBe(true);
  });

  it('shows an error if deleting the conflicted tag itself fails', () => {
    const { component, releasesStub } = setup();
    releasesStub.create.mockReturnValueOnce(throwError(() => ({ status: 400 })));
    component.newTagName.set('v1.0.0');
    component.title.set('First release');
    component.submit();
    releasesStub.deleteTag.mockReturnValueOnce(throwError(() => ({ status: 409 })));

    component.deleteConflictedTagAndRetry();

    expect(component.error()).toContain('v1.0.0');
    expect(releasesStub.create).toHaveBeenCalledTimes(1);
  });

  describe('the dialog', () => {
    const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
    const buttonByText = (root: Element, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);
    const labels = (root: Element) =>
      Array.from(root.querySelectorAll('label'))
        .map((label) => text(label).replace(/ \*$/, ''))
        .filter(Boolean);

    it('explains each flag by a hint the switch is described by', () => {
      const { fixture } = setup();
      const dialog = fixture.nativeElement.querySelector('[role="dialog"]') as HTMLElement;
      const described = (name: string) => {
        const input = Array.from(dialog.querySelectorAll<HTMLInputElement>('input[role="switch"]')).find((i) => text(i.closest('gbt-switch')?.querySelector('label')) === name)!;
        return (input.getAttribute('aria-describedby') ?? '')
          .split(' ')
          .filter(Boolean)
          .map((id) => text(dialog.querySelector(`#${id}`)));
      };

      expect(described('Brouillon')).toEqual(["Visible des mainteneurs seulement, jusqu'à sa publication"]);
      expect(described('Pré-version')).toEqual(['Une version de test, pas encore stable']);
    });

    it('is titled "Nouvelle release" and asks for the tag, its target, the title, the notes and the flags', () => {
      const { fixture } = setup();
      const dialog = fixture.nativeElement.querySelector('[role="dialog"]') as HTMLElement;

      expect(dialog.getAttribute('aria-label')).toBe('Nouvelle release');
      const fieldLabels = labels(dialog);
      for (const label of ['Tag', 'Nom du nouveau tag', 'Branche cible', 'Titre', 'Notes de version', 'Brouillon', 'Pré-version']) {
        expect(fieldLabels).toContain(label);
      }
    });

    it('hides the new-tag name and the target branch once an existing tag is picked', () => {
      const { fixture, component } = setup();
      component.tagChoice.set('v0.9.0');
      fixture.detectChanges();

      const fieldLabels = labels(fixture.nativeElement);
      expect(fieldLabels).not.toContain('Nom du nouveau tag');
      expect(fieldLabels).not.toContain('Branche cible');
    });

    it('says which commit the tag will point at: the tip of the target branch, or the existing tag’s commit', () => {
      const { fixture, component } = setup();
      const target = () => text(fixture.nativeElement.querySelector('.create-release__target'));
      expect(target()).toBe('Le tag pointera sur le dernier commit de main (abc123)');

      component.tagChoice.set('v0.9.0');
      fixture.detectChanges();
      expect(target()).toBe('Ce tag pointe sur le commit deadbee');
    });

    it('closes without creating anything on "Annuler"', () => {
      const { fixture, component, releasesStub } = setup();
      let closed = false;
      component.close.subscribe(() => (closed = true));

      buttonByText(fixture.nativeElement, 'Annuler')!.click();

      expect(closed).toBe(true);
      expect(releasesStub.create).not.toHaveBeenCalled();
    });

    it('creates the release from the "Créer la release" submit button', () => {
      const { fixture, component, releasesStub } = setup();
      component.newTagName.set('v1.0.0');
      component.title.set('First release');
      fixture.detectChanges();

      const submit = buttonByText(fixture.nativeElement, 'Créer la release')!;
      expect(submit.type).toBe('submit');
      submit.click();

      expect(releasesStub.create).toHaveBeenCalledWith('r1', expect.objectContaining({ tagName: 'v1.0.0', targetCommitSha: 'abc123', title: 'First release', draft: true, prerelease: false }));
    });

    it('does not send a second request while the first one is pending', () => {
      const { fixture, component, releasesStub } = setup();
      const pending = new Subject<never>();
      releasesStub.create.mockReturnValue(pending as never);
      component.newTagName.set('v1.0.0');
      component.title.set('First release');

      component.submit();
      component.submit();
      fixture.detectChanges();

      expect(releasesStub.create).toHaveBeenCalledTimes(1);
      expect(fixture.nativeElement.querySelector('button[type="submit"]')!.getAttribute('aria-busy')).toBe('true');
    });

    it('while the request runs: says so, disables "Annuler" and ignores Escape, the backdrop and the close button (a draft is never dropped in flight)', () => {
      const { fixture, component, releasesStub } = setup();
      releasesStub.create.mockReturnValue(new Subject<never>() as never);
      let closed = 0;
      component.close.subscribe(() => closed++);
      component.newTagName.set('v1.0.0');
      component.title.set('First release');

      component.submit();
      fixture.detectChanges();

      const el = fixture.nativeElement as HTMLElement;
      expect(text(el.querySelector('[role="status"]'))).toBe('Création en cours');
      expect(buttonByText(el, 'Annuler')!.disabled).toBe(true);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      el.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      el.querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      buttonByText(el, 'Annuler')!.click();
      expect(closed).toBe(0);
    });

    it('closes from Escape, the backdrop and the close button once idle', () => {
      const { fixture, component } = setup();
      let closed = 0;
      component.close.subscribe(() => closed++);
      const el = fixture.nativeElement as HTMLElement;

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      el.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      el.querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();

      expect(closed).toBe(3);
    });

    it('shows the error in an alert, with the delete-and-retry offer as a danger button, and keeps the draft', () => {
      const { fixture, component, releasesStub } = setup();
      releasesStub.create.mockReturnValueOnce(throwError(() => ({ status: 400 })));
      component.newTagName.set('v1.0.0');
      component.title.set('First release');
      component.submit();
      fixture.detectChanges();

      const alert = fixture.nativeElement.querySelector('.create-release__error [role="alert"]') as HTMLElement;
      expect(text(alert)).toContain('Le tag « v1.0.0 » existe déjà sur un autre commit.');
      const retry = buttonByText(alert, 'Supprimer le tag « v1.0.0 » et réessayer')!;
      expect(retry.classList).toContain('gbt-button--danger');
      expect(component.title()).toBe('First release');
      expect(component.newTagName()).toBe('v1.0.0');
    });

    it('has one primary button: "Créer la release"', () => {
      const { fixture } = setup();
      const primaries = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('.gbt-button--primary'));
      expect(primaries.map(text)).toEqual(['Créer la release']);
    });
  });
});
