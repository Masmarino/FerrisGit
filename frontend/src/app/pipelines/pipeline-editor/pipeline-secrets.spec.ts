import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { PipelineSecrets, SecretUse } from './pipeline-secrets';

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

describe('PipelineSecrets', () => {
  function setup(inputs: { canManage?: boolean; engine?: string | null; missing?: SecretUse[]; existing?: SecretUse[]; suggestion?: string | null } = {}) {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const fixture = TestBed.createComponent(PipelineSecrets);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('canManage', inputs.canManage ?? true);
    fixture.componentRef.setInput('engine', inputs.engine ?? null);
    fixture.componentRef.setInput('missing', inputs.missing ?? []);
    fixture.componentRef.setInput('existing', inputs.existing ?? []);
    fixture.componentRef.setInput('suggestion', inputs.suggestion ?? null);
    fixture.detectChanges();
    const http = TestBed.inject(HttpTestingController);
    return { fixture, http, el: fixture.nativeElement as HTMLElement };
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('lets a maintainer manage the secrets, with the form of the repository settings', () => {
    const { http, el } = setup();
    http.expectOne('/api/repositories/repo-1/ci-variables').flush([]);

    expect(el.querySelector('fg-repository-ci-variables')).not.toBeNull();
  });

  it('only explains, to someone who is not a maintainer, and never asks the server for the list', () => {
    const { el } = setup({ canManage: false });

    expect(text(el)).toContain('Seul un mainteneur');
    expect(el.querySelector('fg-repository-ci-variables')).toBeNull();
  });

  it('warns that Kubernetes jobs do not get the secrets', () => {
    const { http, el } = setup({ engine: 'kubernetes' });
    http.expectOne('/api/repositories/repo-1/ci-variables').flush([]);

    expect(text(el.querySelector('gbt-alert'))).toContain('Kubernetes');
  });

  it('says nothing about Kubernetes with Docker runners', () => {
    const { http, el } = setup({ engine: 'docker-runners' });
    http.expectOne('/api/repositories/repo-1/ci-variables').flush([]);

    expect(text(el)).not.toContain('Kubernetes');
  });

  it('lists the secrets the jobs read and the repository lacks, and starts the form with the one asked for', () => {
    const { fixture, http, el } = setup({ missing: [{ name: 'DEPLOY_TOKEN', jobs: ['deploy'] }] });
    http.expectOne('/api/repositories/repo-1/ci-variables').flush([]);

    expect(text(el.querySelector('[data-missing="DEPLOY_TOKEN"]'))).toContain('lu par deploy');
    el.querySelector<HTMLButtonElement>('[data-missing="DEPLOY_TOKEN"] button')!.click();
    fixture.detectChanges();

    expect(fixture.componentInstance['chosen']()).toBe('DEPLOY_TOKEN');
  });

  it('shows where each existing secret is read, and that one is not read at all', () => {
    const { http, el } = setup({ existing: [{ name: 'A', jobs: ['build', 'deploy'] }, { name: 'B', jobs: [] }] });
    http.expectOne('/api/repositories/repo-1/ci-variables').flush([]);

    expect(text(el.querySelector('[data-existing="A"]'))).toContain('lu par build, deploy');
    expect(text(el.querySelector('[data-existing="B"]'))).toContain('Aucun job ne le lit');
  });

  it('tells its host when the list of secrets changed', () => {
    const { fixture, http } = setup();
    http.expectOne('/api/repositories/repo-1/ci-variables').flush([]);
    const changed = vi.fn();
    fixture.componentInstance.changed.subscribe(changed);

    // The inner component reports through its output: trigger it the way the host would.
    const inner = fixture.debugElement.query((d) => d.name === 'fg-repository-ci-variables').componentInstance;
    inner.changed.emit();

    expect(changed).toHaveBeenCalledTimes(1);
  });
});
