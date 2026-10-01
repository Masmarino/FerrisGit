import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter, ActivatedRoute, UrlSegment } from '@angular/router';
import { By } from '@angular/platform-browser';
import { BehaviorSubject, Subject } from 'rxjs';
import { RepositoryPathResolver } from './repository-path-resolver';
import { RepositoryDetail } from '../repository-detail/repository-detail';
import { RepositoryContextService } from '../repository-context.service';

// `repositories/**` makes `ActivatedRoute.url` include the literal `repositories` prefix, which must be
// stripped before `resolve()` and before the `path` input reaches children.
function urlSegments(paths: string[]): UrlSegment[] {
  return paths.map((path) => new UrlSegment(path, {}));
}

describe('RepositoryPathResolver', () => {
  function setup() {
    const url$ = new Subject<UrlSegment[]>();
    // `snapshot.fragment`: the tree view reads it (the `#cloner` fragment) once its repository loads.
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: ActivatedRoute, useValue: { url: url$, snapshot: { fragment: null } } }],
    });
    const fixture = TestBed.createComponent(RepositoryPathResolver);
    const http = TestBed.inject(HttpTestingController);
    return { fixture, http, url$ };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('requests /api/resolve/{path} without the route\'s own "repositories" prefix segment', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo']));

    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });

    // Resolving enters the repository context, which fetches the role: flush it so verify() passes.
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });
  });

  it('splits at the "/-/" marker: segments before it resolve the repository, segments after it drive the sub-page view', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines']));

    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    const view = fixture.componentInstance['view']();
    expect(view.kind).toBe('pipelines');
    expect((view as { repositoryId: string }).repositoryId).toBe('repo-1');
    expect((view as { path: string[] }).path).toEqual(['admin', 'my-repo']);
  });

  it('resolves a pipeline URL to the summary view (no job)', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    expect(fixture.componentInstance['view']()).toMatchObject({ kind: 'pipelineDetail', id: 'pipe-1', jobId: null });
  });

  it('resolves a pipeline job URL to the job view', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1', 'jobs', 'job-9']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    expect(fixture.componentInstance['view']()).toMatchObject({ kind: 'pipelineDetail', id: 'pipe-1', jobId: 'job-9' });
  });

  it('treats a ".../jobs" URL without a job id as the summary', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1', 'jobs']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    expect(fixture.componentInstance['view']()).toMatchObject({ kind: 'pipelineDetail', jobId: null });
  });

  it('passes a "path" to the repository child component that does not include the "repositories" prefix', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'owner' });

    fixture.detectChanges();

    // Rendering RepositoryDetail fires its own requests: flush them so verify() passes.
    http.match(() => true).forEach((req) => req.flush([]));

    const child = fixture.debugElement.query(By.directive(RepositoryDetail));
    expect(child).not.toBeNull();
    expect(child.componentInstance.path()).toEqual(['admin', 'my-repo']);
  });

  it('resolves a "/-/tree/{ref}/{...path}" sub-page to the tree view with the ref and remaining path', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'tree', 'main', 'src', 'lib']));

    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    const view = fixture.componentInstance['view']();
    expect(view.kind).toBe('tree');
    expect((view as { repositoryId: string }).repositoryId).toBe('repo-1');
    expect((view as { ref: string }).ref).toBe('main');
    expect((view as { treePath: string[] }).treePath).toEqual(['src', 'lib']);
  });

  it('resolves a "/-/blob/{ref}/{...path}" sub-page to the blob view with the ref and remaining path', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'blob', 'main', 'src', 'lib.rs']));

    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    const view = fixture.componentInstance['view']();
    expect(view.kind).toBe('blob');
    expect((view as { repositoryId: string }).repositoryId).toBe('repo-1');
    expect((view as { ref: string }).ref).toBe('main');
    expect((view as { blobPath: string[] }).blobPath).toEqual(['src', 'lib.rs']);
  });

  it('falls back to "HEAD" for a "/-/tree" sub-page with no ref segment', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'tree']));

    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    const view = fixture.componentInstance['view']();
    expect(view.kind).toBe('tree');
    expect((view as { ref: string }).ref).toBe('HEAD');
    expect((view as { treePath: string[] }).treePath).toEqual([]);
  });

  it('resolves a "/-/blob" sub-page with no ref or path to notFound', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'blob']));

    // `notFound` never enters the repository context, so only the resolve call fires here.
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });

    expect(fixture.componentInstance['view']().kind).toBe('notFound');
    http.expectNone('/api/repositories/by-id/repo-1');
  });

  it('resolves a "/-/blob/{ref}" sub-page with a ref but no path to notFound', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'blob', 'main']));

    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });

    expect(fixture.componentInstance['view']().kind).toBe('notFound');
    http.expectNone('/api/repositories/by-id/repo-1');
  });

  it('resolves exactly once on the initial load (no duplicate request from a stray snapshot call)', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo']));

    // A redundant second resolve() would make `expectOne` fail.
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });
  });

  it('keeps the pipeline view (no loading state, no new resolve request) when only the job changes within the same pipeline', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    // No `detectChanges()` here: rendering the real pipeline shell would need route and HTTP stubs.
    const view = fixture.componentInstance['view'];
    // Records every write to the signal: an `effect` would only see the settled value and miss a `loading` reset followed by a synchronous re-set.
    const seen: string[] = [];
    const originalSet = view.set.bind(view);
    const setSpy = vi.spyOn(view, 'set').mockImplementation((value) => {
      seen.push(value.kind);
      originalSet(value);
    });

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1', 'jobs', 'job-9']));
    http.expectNone('/api/resolve/admin/my-repo');
    expect(view()).toMatchObject({ kind: 'pipelineDetail', id: 'pipe-1', jobId: 'job-9', repositoryId: 'repo-1', path: ['admin', 'my-repo'] });
    expect(seen).not.toContain('loading');

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1']));
    http.expectNone('/api/resolve/admin/my-repo');
    expect(view()).toMatchObject({ kind: 'pipelineDetail', id: 'pipe-1', jobId: null });
    expect(seen).not.toContain('loading');
    expect(seen).toEqual(['pipelineDetail', 'pipelineDetail']);
    setSpy.mockRestore();
  });

  it('still resolves (with the loading state) when the pipeline id changes', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-2']));
    expect(fixture.componentInstance['view']().kind).toBe('loading');
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    expect(fixture.componentInstance['view']()).toMatchObject({ kind: 'pipelineDetail', id: 'pipe-2' });
  });

  it('still resolves when leaving the pipeline for another sub-page of the same repository', () => {
    const { fixture, http, url$ } = setup();
    fixture.detectChanges();

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines', 'pipe-1']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    url$.next(urlSegments(['repositories', 'admin', 'my-repo', '-', 'pipelines']));
    http.expectOne('/api/resolve/admin/my-repo').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    expect(fixture.componentInstance['view']().kind).toBe('pipelines');
  });
});

describe('RepositoryPathResolver (repository context)', () => {
  function setup(segments: string[]) {
    const url$ = new BehaviorSubject<UrlSegment[]>(urlSegments(segments));
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([]),
        { provide: ActivatedRoute, useValue: { url: url$, snapshot: { fragment: null } } },
      ],
    });
    const fixture = TestBed.createComponent(RepositoryPathResolver);
    const http = TestBed.inject(HttpTestingController);
    const context = TestBed.inject(RepositoryContextService);
    return { fixture, http, context };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('enters the repository context when resolving the repository overview', () => {
    const { fixture, http, context } = setup(['repositories', 'acme', 'widget']);
    fixture.detectChanges();

    http.expectOne('/api/resolve/acme/widget').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    expect(context.current()).toEqual({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: 'reader', ancestors: [], groupId: null });
  });

  it('carries ancestors and groupId into the repository context for a group-owned repository', () => {
    const { fixture, http, context } = setup(['repositories', 'acme', 'backend-team', 'widget']);
    fixture.detectChanges();

    http.expectOne('/api/resolve/acme/backend-team/widget').flush({
      type: 'groupRepository',
      repositoryId: 'repo-1',
      chain: [
        { id: 'group-acme', name: 'acme' },
        { id: 'group-backend-team', name: 'backend-team' },
      ],
    });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    expect(context.current()?.groupId).toBe('group-backend-team');
    expect(context.current()?.ancestors).toEqual([
      { label: 'acme', link: ['/repositories', 'acme'] },
      { label: 'backend-team', link: ['/repositories', 'acme', 'backend-team'] },
    ]);
  });

  it('has empty ancestors and a null groupId for a personal-namespace repository', () => {
    const { fixture, http, context } = setup(['repositories', 'acme', 'widget']);
    fixture.detectChanges();

    http.expectOne('/api/resolve/acme/widget').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    expect(context.current()?.groupId).toBeNull();
    expect(context.current()?.ancestors).toEqual([]);
  });

  it('enters the repository context for a repository sub-page', () => {
    const { fixture, http, context } = setup(['repositories', 'acme', 'widget', '-', 'issues']);
    fixture.detectChanges();

    http.expectOne('/api/resolve/acme/widget').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });

    expect(context.current()?.repositoryId).toBe('repo-1');
    expect(context.current()?.path).toEqual(['acme', 'widget']);
  });

  it('leaves the repository context when resolving a group', () => {
    const { fixture, http, context } = setup(['repositories', 'acme']);
    context.enter('repo-1', ['acme', 'widget'], [], null);
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'reader' });
    fixture.detectChanges();

    http.expectOne('/api/resolve/acme').flush({ type: 'group', groupId: 'group-1', chain: [], role: 'maintainer' });

    expect(context.current()).toBeNull();
  });

  it('leaves the repository context when destroyed', () => {
    const { fixture, http, context } = setup(['repositories', 'acme', 'widget']);
    fixture.detectChanges();
    http.expectOne('/api/resolve/acme/widget').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'owner' });
    expect(context.current()).not.toBeNull();

    fixture.destroy();

    expect(context.current()).toBeNull();
  });

  it('leaves the repository context when the resolve request itself fails (repository does not exist)', () => {
    const { fixture, http, context } = setup(['repositories', 'acme', 'deleted-repo']);
    context.enter('repo-1', ['acme', 'widget'], [], null);
    http.expectOne('/api/repositories/by-id/repo-1').flush({ role: 'owner' });
    fixture.detectChanges();

    http.expectOne('/api/resolve/acme/deleted-repo').flush('not found', { status: 404, statusText: 'Not Found' });

    expect(context.current()).toBeNull();
  });

  it('says "Introuvable." in an empty state when the resolve request fails, and nothing before that', () => {
    const { fixture, http } = setup(['repositories', 'acme', 'deleted-repo']);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('gbt-empty-state')).toBeNull();
    expect(fixture.nativeElement.querySelector('gbt-spinner')).not.toBeNull();

    http.expectOne('/api/resolve/acme/deleted-repo').flush('not found', { status: 404, statusText: 'Not Found' });
    fixture.detectChanges();

    const emptyState: HTMLElement = fixture.nativeElement.querySelector('gbt-empty-state');
    expect(emptyState.textContent?.trim()).toBe('Introuvable.');
    expect(fixture.nativeElement.querySelector('gbt-spinner')).toBeNull();
  });
});
