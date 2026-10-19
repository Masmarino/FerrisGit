import { of, throwError } from 'rxjs';
import { parseCheckNames, RepositoryProtectedBranches } from './repository-protected-branches';
import { ProtectedBranchRule, RepositorySettingsService } from '../repository-settings.service';
import { createSettingsSection } from '../settings-section-testing';

const RULES: ProtectedBranchRule[] = [
  { id: 'p1', pattern: 'main', requiredApprovals: 0, requiredChecks: [], createdAt: '2026-01-01T00:00:00Z' },
  { id: 'p2', pattern: 'release/*', requiredApprovals: 2, requiredChecks: ['pipeline', 'pipeline/test'], createdAt: '2026-01-02T00:00:00Z' },
];

describe('RepositoryProtectedBranches', () => {
  function setup() {
    const repositorySettingsStub = {
      listProtectedBranches: vi.fn(() => of<ProtectedBranchRule[]>([])),
      protectBranch: vi.fn(() => of<ProtectedBranchRule>(RULES[0])),
      unprotectBranch: vi.fn(() => of<void>(undefined)),
      updateProtectedBranch: vi.fn(() => of<ProtectedBranchRule>(RULES[1])),
    };
    return { ...createSettingsSection(RepositoryProtectedBranches, [{ provide: RepositorySettingsService, useValue: repositorySettingsStub }]), repositorySettingsStub };
  }

  it('lists the protected patterns', () => {
    const { fixture, repositorySettingsStub } = setup();
    repositorySettingsStub.listProtectedBranches.mockReturnValue(of(RULES));
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent;
    expect(repositorySettingsStub.listProtectedBranches).toHaveBeenCalledWith('repo-1');
    expect(text).toContain('main');
    expect(text).toContain('release/*');
  });

  it('shows what a merge into each rule\'s branches needs', () => {
    const { fixture, repositorySettingsStub } = setup();
    repositorySettingsStub.listProtectedBranches.mockReturnValue(of(RULES));
    fixture.detectChanges();

    const rows = Array.from<HTMLElement>(fixture.nativeElement.querySelectorAll('li')).map((li) => li.textContent?.replace(/\s+/g, ' ').trim());
    expect(rows[0]).toBe('main');
    expect(rows[1]).toContain('2 approbations');
    expect(rows[1]).toContain('pipeline/test');
  });

  it('protects a pattern with the approvals and checks it requires', () => {
    const { component, repositorySettingsStub } = setup();

    component['newPattern'].set('main');
    component['newApprovals'].set('2');
    component['newChecks'].set(' pipeline, pipeline/test ,, pipeline');
    component.protect();

    expect(repositorySettingsStub.protectBranch).toHaveBeenCalledWith('repo-1', 'main', 2, ['pipeline', 'pipeline/test']);
  });

  it('edits the requirements of an existing rule, its pattern staying as it is', () => {
    const { component, repositorySettingsStub, toastStub } = setup();

    component.edit(RULES[1]);
    expect(component['newPattern']()).toBe('release/*');
    expect(component['newChecks']()).toBe('pipeline, pipeline/test');
    component['newApprovals'].set('1');
    component.protect();

    expect(repositorySettingsStub.updateProtectedBranch).toHaveBeenCalledWith('repo-1', 'p2', { requiredApprovals: 1, requiredChecks: ['pipeline', 'pipeline/test'] });
    expect(repositorySettingsStub.protectBranch).not.toHaveBeenCalled();
    expect(component['editing']()).toBeNull();
    expect(toastStub.show).toHaveBeenCalledWith('Règle enregistrée.');
  });

  it('refuses a number of approvals the server would refuse', () => {
    const { component, repositorySettingsStub } = setup();

    component['newPattern'].set('main');
    component['newApprovals'].set('-1');
    component.protect();

    expect(component['approvalsProblem']()).toBe('Un nombre entre 0 et 100.');
    expect(repositorySettingsStub.protectBranch).not.toHaveBeenCalled();
  });

  it('reads check names from a comma-separated list', () => {
    expect(parseCheckNames(' a, b ,,a, c/d ')).toEqual(['a', 'b', 'c/d']);
    expect(parseCheckNames('')).toEqual([]);
  });

  it('says what an empty list means', () => {
    const { fixture } = setup();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Aucune branche protégée');
  });

  it('protects the trimmed pattern, clears the form and reloads the list', () => {
    const { component, repositorySettingsStub, toastStub } = setup();

    component['newPattern'].set('  release/*  ');
    component.protect();

    expect(repositorySettingsStub.protectBranch).toHaveBeenCalledWith('repo-1', 'release/*', 0, []);
    expect(component['newPattern']()).toBe('');
    expect(repositorySettingsStub.listProtectedBranches).toHaveBeenCalledTimes(1);
    expect(toastStub.show).toHaveBeenCalledWith('Branche protégée.');
  });

  it('does not send a pattern the server would refuse, and says why', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    fixture.detectChanges();

    component['newPattern'].set('refs/heads/main');
    fixture.detectChanges();
    component.protect();

    expect(component['patternProblem']()).toContain('sans refs/heads/');
    expect(repositorySettingsStub.protectBranch).not.toHaveBeenCalled();
  });

  it('tells a pattern that is already protected apart from another failure', () => {
    const { component, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.protectBranch.mockReturnValueOnce(throwError(() => ({ status: 409 })));
    component['newPattern'].set('main');
    component.protect();
    expect(toastStub.show).toHaveBeenLastCalledWith('Ce motif est déjà protégé.', 'error');

    repositorySettingsStub.protectBranch.mockReturnValueOnce(throwError(() => ({ status: 500 })));
    component.protect();
    expect(toastStub.show).toHaveBeenLastCalledWith('Impossible de protéger la branche.', 'error');
  });

  it('removes a protection after confirmation', () => {
    const { component, fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.listProtectedBranches.mockReturnValue(of(RULES));
    fixture.detectChanges();

    const button = fixture.nativeElement.querySelector('[aria-label="Ne plus protéger main"] button, button[aria-label="Ne plus protéger main"]') as HTMLButtonElement;
    button.click();
    fixture.detectChanges();
    expect(component['rulePendingDelete']()).toEqual(RULES[0]);

    component['unprotect']('p1');
    expect(repositorySettingsStub.unprotectBranch).toHaveBeenCalledWith('repo-1', 'p1');
    expect(component['rulePendingDelete']()).toBeNull();
    expect(toastStub.show).toHaveBeenCalledWith("La branche n'est plus protégée.");
  });

  it('surfaces an error when the list cannot be loaded', () => {
    const { fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.listProtectedBranches.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain("Les branches protégées n'ont pas pu être chargées.");
    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
  });
});
