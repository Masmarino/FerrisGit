import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { MilestonesService } from './milestones.service';

describe('MilestonesService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), MilestonesService] });
    return { service: TestBed.inject(MilestonesService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists milestones for a repository', () => {
    const { service, http } = setup();
    service.listForRepository('repo-1').subscribe();
    http.expectOne({ url: '/api/repositories/repo-1/milestones', method: 'GET' }).flush([]);
  });

  it('lists milestones for a group', () => {
    const { service, http } = setup();
    service.listForGroup('group-1').subscribe();
    http.expectOne({ url: '/api/groups/group-1/milestones', method: 'GET' }).flush([]);
  });

  it('creates a repository-scoped milestone', () => {
    const { service, http } = setup();
    service.create({ repositoryId: 'repo-1' }, 'v1.0', 'First release', '2026-03-01T00:00:00Z').subscribe();
    const req = http.expectOne({ url: '/api/repositories/repo-1/milestones', method: 'POST' });
    expect(req.request.body).toEqual({ title: 'v1.0', description: 'First release', dueDate: '2026-03-01T00:00:00Z' });
    req.flush({ id: 'milestone-1', title: 'v1.0', description: 'First release', dueDate: '2026-03-01T00:00:00Z', state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' });
  });

  it('creates a group-scoped milestone', () => {
    const { service, http } = setup();
    service.create({ groupId: 'group-1' }, 'v1.0', 'First release', null).subscribe();
    http.expectOne({ url: '/api/groups/group-1/milestones', method: 'POST' }).flush({});
  });

  it('updates a milestone', () => {
    const { service, http } = setup();
    service.update('milestone-1', 'v1.0', 'First release', null, 'closed').subscribe();
    const req = http.expectOne({ url: '/api/milestones/milestone-1', method: 'PATCH' });
    expect(req.request.body).toEqual({ title: 'v1.0', description: 'First release', dueDate: null, state: 'closed' });
    req.flush({});
  });

  it('deletes a milestone', () => {
    const { service, http } = setup();
    service.delete('milestone-1').subscribe();
    http.expectOne({ url: '/api/milestones/milestone-1', method: 'DELETE' }).flush(null);
  });
});
