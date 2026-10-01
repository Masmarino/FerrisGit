import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { GroupsService } from './groups.service';

describe('GroupsService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), GroupsService] });
    return { service: TestBed.inject(GroupsService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('creates a root group', () => {
    const { service, http } = setup();
    let result: unknown;
    service.createRoot('Acme', 'An acme group').subscribe((g) => (result = g));
    const req = http.expectOne({ url: '/api/groups', method: 'POST' });
    expect(req.request.body).toEqual({ name: 'Acme', description: 'An acme group' });
    const group = { id: 'group-1', parentGroupId: null, name: 'Acme', description: 'An acme group', createdAt: '2026-01-01T00:00:00Z' };
    req.flush(group);
    expect(result).toEqual(group);
  });

  it('creates a subgroup', () => {
    const { service, http } = setup();
    let result: unknown;
    service.createSubgroup('group-1', 'Sub', 'A subgroup').subscribe((g) => (result = g));
    const req = http.expectOne({ url: '/api/groups/group-1/subgroups', method: 'POST' });
    expect(req.request.body).toEqual({ name: 'Sub', description: 'A subgroup' });
    const group = { id: 'group-2', parentGroupId: 'group-1', name: 'Sub', description: 'A subgroup', createdAt: '2026-01-01T00:00:00Z' };
    req.flush(group);
    expect(result).toEqual(group);
  });

  it('lists writable groups', () => {
    const { service, http } = setup();
    let result: unknown;
    service.listWritable().subscribe((g) => (result = g));
    const writable = [{ id: 'group-1', path: 'acme' }];
    http.expectOne({ url: '/api/groups/writable', method: 'GET' }).flush(writable);
    expect(result).toEqual(writable);
  });

  it('lists groups the caller is a member of', () => {
    const { service, http } = setup();
    let result: unknown;
    service.listMember().subscribe((g) => (result = g));
    const membership = [{ id: 'group-1', path: 'acme', role: 'maintainer' }];
    http.expectOne({ url: '/api/groups/member', method: 'GET' }).flush(membership);
    expect(result).toEqual(membership);
  });

  it('deletes a group', () => {
    const { service, http } = setup();
    service.delete('group-1').subscribe();
    http.expectOne({ url: '/api/groups/group-1', method: 'DELETE' }).flush(null);
  });

  it('lists child groups', () => {
    const { service, http } = setup();
    let result: unknown;
    service.listChildren('group-1').subscribe((g) => (result = g));
    const children = [{ id: 'group-2', parentGroupId: 'group-1', name: 'Sub', description: '', createdAt: '2026-01-01T00:00:00Z' }];
    http.expectOne({ url: '/api/groups/group-1/children', method: 'GET' }).flush(children);
    expect(result).toEqual(children);
  });

  it('lists group members', () => {
    const { service, http } = setup();
    let result: unknown;
    service.listMembers('group-1').subscribe((m) => (result = m));
    const members = [{ userId: 'user-1', username: 'alice', role: 'maintainer', createdAt: '2026-01-01T00:00:00Z' }];
    http.expectOne({ url: '/api/groups/group-1/members', method: 'GET' }).flush(members);
    expect(result).toEqual(members);
  });

  it('adds a member', () => {
    const { service, http } = setup();
    service.addMember('group-1', 'alice', 'contributor').subscribe();
    const req = http.expectOne({ url: '/api/groups/group-1/members', method: 'POST' });
    expect(req.request.body).toEqual({ username: 'alice', role: 'contributor' });
    req.flush(null);
  });

  it('sets a member role', () => {
    const { service, http } = setup();
    service.setMemberRole('group-1', 'alice', 'maintainer').subscribe();
    const req = http.expectOne({ url: '/api/groups/group-1/members/alice', method: 'PATCH' });
    expect(req.request.body).toEqual({ role: 'maintainer' });
    req.flush(null);
  });

  it('removes a member', () => {
    const { service, http } = setup();
    service.removeMember('group-1', 'alice').subscribe();
    http.expectOne({ url: '/api/groups/group-1/members/alice', method: 'DELETE' }).flush(null);
  });
});
