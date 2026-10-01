import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { GbtInput, GbtToastService } from '@masmarino/gabarit';
import { of, Subject, throwError } from 'rxjs';
import { RepositoryCiVariables } from './repository-ci-variables';
import { CiVariableSummary, RepositorySettingsService } from '../repository-settings.service';

describe('RepositoryCiVariables', () => {
  function setup() {
    const repositorySettingsStub = {
      listCiVariables: vi.fn(() => of<CiVariableSummary[]>([])),
      setCiVariable: vi.fn(() => of<CiVariableSummary>({ id: 'v1', key: 'KEY', masked: true })),
      deleteCiVariable: vi.fn(() => of<void>(undefined)),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: RepositorySettingsService, useValue: repositorySettingsStub },
        { provide: GbtToastService, useValue: toastStub },
      ],
    });
    const fixture = TestBed.createComponent(RepositoryCiVariables);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    return { fixture, component: fixture.componentInstance, repositorySettingsStub, toastStub };
  }

  it('lists configured CI variables, masking their value', () => {
    const { fixture, repositorySettingsStub } = setup();
    repositorySettingsStub.listCiVariables.mockReturnValue(of([{ id: 'v1', key: 'API_KEY', masked: true }]));
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('API_KEY');
    expect(fixture.nativeElement.textContent).toContain('••••••••');
  });

  it('sets the variable name in monospace (an identifier), not the value', () => {
    const { fixture } = setup();
    fixture.detectChanges();

    const [name, value] = fixture.debugElement.queryAll(By.directive(GbtInput)).map((d) => d.componentInstance as GbtInput);
    expect(name.label()).toBe('Nom');
    expect(name.mono()).toBe(true);
    expect(value.label()).toBe('Valeur');
    expect(value.mono()).toBe(false);
  });

  it('surfaces an error when loading CI variables fails', () => {
    const { fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.listCiVariables.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
  });

  it('adds a variable with the given key and value, and clears the form', () => {
    const { component, repositorySettingsStub } = setup();

    component['newVariableKey'].set('API_KEY');
    component['newVariableValue'].set('secret');
    component.addVariable();

    expect(repositorySettingsStub.setCiVariable).toHaveBeenCalledWith('repo-1', 'API_KEY', 'secret', true);
    expect(component['newVariableKey']()).toBe('');
    expect(component['newVariableValue']()).toBe('');
    // `refresh()` re-fetched only this section's list (`ngOnInit` never ran here).
    expect(repositorySettingsStub.listCiVariables).toHaveBeenCalledTimes(1);
  });

  it('does not add a variable when the key or value is missing', () => {
    const { component, repositorySettingsStub } = setup();

    component['newVariableKey'].set('');
    component['newVariableValue'].set('secret');
    component.addVariable();

    expect(repositorySettingsStub.setCiVariable).not.toHaveBeenCalled();
  });

  describe('layout', () => {
    const VARIABLES: CiVariableSummary[] = [
      { id: 'v1', key: 'DATABASE_URL', masked: true },
      { id: 'v2', key: 'RUST_LOG', masked: false },
    ];
    const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
    const button = (root: Element, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label || b.getAttribute('aria-label') === label);

    function loaded(variables: CiVariableSummary[] = VARIABLES) {
      const ctx = setup();
      ctx.repositorySettingsStub.listCiVariables.mockReturnValue(of(variables));
      ctx.fixture.detectChanges();
      return { ...ctx, el: ctx.fixture.nativeElement as HTMLElement };
    }

    it('has a creation card and a list card with the number of variables', () => {
      const { el } = loaded();

      const headings = Array.from(el.querySelectorAll('gbt-card h2')).map(text);
      expect(headings).toEqual(['Ajouter une variable', 'Variables 2']);
      expect(el.querySelectorAll('gbt-card .gbt-card__description')).toHaveLength(2);
    });

    it('lists each variable as a row: its key, then whether its value is masked in the job logs (text, not colour alone)', () => {
      const { el } = loaded();

      const rows = Array.from(el.querySelectorAll('ul > li > gbt-list-row'));
      expect(rows.map((row) => text(row.querySelector('.repository-ci-variables__key')))).toEqual(['DATABASE_URL', 'RUST_LOG']);
      expect(text(rows[0].querySelector('.repository-ci-variables__value'))).toContain('Masquée');
      expect(text(rows[1].querySelector('.repository-ci-variables__value'))).toContain('Visible dans les journaux');
    });

    it('shows skeleton rows while the variables load', () => {
      const { fixture, repositorySettingsStub } = setup();
      repositorySettingsStub.listCiVariables.mockReturnValue(new Subject<CiVariableSummary[]>());
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;

      expect(el.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(3);
      // One polite status, outside any aria-busy region (a busy ancestor can hold the announcement back).
      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des variables…');
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      expect(el.querySelector('gbt-list-row')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toBeUndefined();
    });

    it('explains the empty list', () => {
      const { el } = loaded([]);

      expect(el.querySelector('gbt-list-row')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain('Aucune variable définie');
      expect(text(el.querySelector('gbt-card:last-of-type h2'))).toBe('Variables 0');
    });

    it('shows a failed state with a retry button when the list cannot be loaded', () => {
      const { fixture, repositorySettingsStub } = setup();
      repositorySettingsStub.listCiVariables.mockReturnValue(throwError(() => ({ status: 500 })));
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les variables n'ont pas pu être chargées");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      // The error toast announces the failure, so the inline block stays silent (one live region, not two).
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);

      repositorySettingsStub.listCiVariables.mockReturnValue(of(VARIABLES));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it('adds the variable when the form is submitted (Enter in a field)', () => {
      const { component, el, repositorySettingsStub } = loaded();
      component['newVariableKey'].set('API_KEY');
      component['newVariableValue'].set('secret');

      el.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));

      expect(repositorySettingsStub.setCiVariable).toHaveBeenCalledWith('repo-1', 'API_KEY', 'secret', true);
    });

    const dialog = (el: HTMLElement) => el.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');

    it('confirms before deleting, without making the user retype the name: confirming deletes that variable and drops its row', () => {
      const { fixture, el, repositorySettingsStub, toastStub } = loaded();

      expect(dialog(el)).toBeNull();
      button(el.querySelectorAll('gbt-list-row')[1], 'Supprimer la variable RUST_LOG')!.click();
      fixture.detectChanges();

      const modal = dialog(el)!;
      expect(modal.getAttribute('aria-label')).toBe('Supprimer la variable');
      expect(text(modal)).toContain('RUST_LOG');
      expect(modal.querySelector('input')).toBeNull();
      expect(button(modal, 'Supprimer')!.disabled).toBe(false);
      expect(repositorySettingsStub.deleteCiVariable).not.toHaveBeenCalled();

      repositorySettingsStub.listCiVariables.mockReturnValue(of([VARIABLES[0]]));
      button(modal, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(repositorySettingsStub.deleteCiVariable).toHaveBeenCalledExactlyOnceWith('repo-1', 'v2');
      expect(dialog(el)).toBeNull();
      expect(Array.from(el.querySelectorAll('.repository-ci-variables__key')).map(text)).toEqual(['DATABASE_URL']);
      expect(toastStub.show).toHaveBeenCalledWith('Variable supprimée.');
    });

    it('keeps the confirmation open and busy while the deletion runs: a second click sends nothing', () => {
      const { fixture, el, repositorySettingsStub } = loaded();
      const pending = new Subject<void>();
      repositorySettingsStub.deleteCiVariable.mockReturnValue(pending);

      button(el.querySelectorAll('gbt-list-row')[0], 'Supprimer la variable DATABASE_URL')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(dialog(el)!.querySelector('.gbt-button--danger')!.getAttribute('aria-busy')).toBe('true');
      dialog(el)!.querySelector<HTMLButtonElement>('.gbt-button--danger')!.click();
      button(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();
      expect(repositorySettingsStub.deleteCiVariable).toHaveBeenCalledTimes(1);
      expect(dialog(el)).not.toBeNull();

      pending.next();
      pending.complete();
      fixture.detectChanges();
      expect(dialog(el)).toBeNull();
    });

    it('closes the confirmation and says so when deleting fails, keeping the row', () => {
      const { fixture, el, repositorySettingsStub, toastStub } = loaded();
      repositorySettingsStub.deleteCiVariable.mockReturnValue(throwError(() => ({ status: 500 })));

      button(el.querySelectorAll('gbt-list-row')[0], 'Supprimer la variable DATABASE_URL')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(repositorySettingsStub.deleteCiVariable).toHaveBeenCalledWith('repo-1', 'v1');
      expect(dialog(el)).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de supprimer la variable.', 'error');
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });
  });
});
