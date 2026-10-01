import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { LabelsService } from './labels.service';

describe('LabelsService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), LabelsService] });
    return { service: TestBed.inject(LabelsService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists labels for a repository', () => {
    const { service, http } = setup();
    service.listForRepository('repo-1').subscribe();
    http.expectOne({ url: '/api/repositories/repo-1/labels', method: 'GET' }).flush([]);
  });

  it('lists labels for a group', () => {
    const { service, http } = setup();
    service.listForGroup('group-1').subscribe();
    http.expectOne({ url: '/api/groups/group-1/labels', method: 'GET' }).flush([]);
  });

  it('creates a repository-scoped label', () => {
    const { service, http } = setup();
    service.create({ repositoryId: 'repo-1' }, 'Bug', '#dc2626').subscribe();
    const req = http.expectOne({ url: '/api/repositories/repo-1/labels', method: 'POST' });
    expect(req.request.body).toEqual({ name: 'Bug', color: '#dc2626' });
    req.flush({ id: 'label-1', name: 'Bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' });
  });

  it('creates a group-scoped label', () => {
    const { service, http } = setup();
    service.create({ groupId: 'group-1' }, 'Bug', '#dc2626').subscribe();
    http.expectOne({ url: '/api/groups/group-1/labels', method: 'POST' }).flush({});
  });

  it('updates a label', () => {
    const { service, http } = setup();
    service.update('label-1', 'Bug', '#dc2626').subscribe();
    const req = http.expectOne({ url: '/api/labels/label-1', method: 'PATCH' });
    expect(req.request.body).toEqual({ name: 'Bug', color: '#dc2626' });
    req.flush({});
  });

  it('replaces an issue’s label set', () => {
    const { service, http } = setup();
    service.setForIssue('repo-1', 3, ['label-1', 'label-2']).subscribe();
    const req = http.expectOne({ url: '/api/repositories/repo-1/issues/3/labels', method: 'PUT' });
    expect(req.request.body).toEqual({ labelIds: ['label-1', 'label-2'] });
    req.flush([]);
  });

  it('replaces a merge request’s label set', () => {
    const { service, http } = setup();
    service.setForMergeRequest('mr-1', ['label-1']).subscribe();
    const req = http.expectOne({ url: '/api/merge-requests/mr-1/labels', method: 'PUT' });
    expect(req.request.body).toEqual({ labelIds: ['label-1'] });
    req.flush([]);
  });

  it('deletes a label', () => {
    const { service, http } = setup();
    service.delete('label-1').subscribe();
    http.expectOne({ url: '/api/labels/label-1', method: 'DELETE' }).flush(null);
  });
});
