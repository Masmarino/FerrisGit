import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface Group {
  id: string;
  parentGroupId: string | null;
  name: string;
  description: string;
  createdAt: string;
}

export interface WritableGroup {
  id: string;
  path: string;
}

export interface GroupMembership {
  id: string;
  path: string;
  role: 'reader' | 'contributor' | 'maintainer';
}

export interface GroupMember {
  userId: string;
  username: string;
  role: 'reader' | 'contributor' | 'maintainer';
  createdAt: string;
}

@Injectable({ providedIn: 'root' })
export class GroupsService {
  private http = inject(HttpClient);

  createRoot(name: string, description: string) {
    return this.http.post<Group>('/api/groups', { name, description });
  }

  createSubgroup(parentId: string, name: string, description: string) {
    return this.http.post<Group>(`/api/groups/${parentId}/subgroups`, { name, description });
  }

  listWritable() {
    return this.http.get<WritableGroup[]>('/api/groups/writable');
  }

  listMember() {
    return this.http.get<GroupMembership[]>('/api/groups/member');
  }

  delete(groupId: string) {
    return this.http.delete<void>(`/api/groups/${groupId}`);
  }

  listChildren(groupId: string) {
    return this.http.get<Group[]>(`/api/groups/${groupId}/children`);
  }

  listMembers(groupId: string) {
    return this.http.get<GroupMember[]>(`/api/groups/${groupId}/members`);
  }

  addMember(groupId: string, username: string, role: string) {
    return this.http.post<void>(`/api/groups/${groupId}/members`, { username, role });
  }

  setMemberRole(groupId: string, username: string, role: string) {
    return this.http.patch<void>(`/api/groups/${groupId}/members/${username}`, { role });
  }

  removeMember(groupId: string, username: string) {
    return this.http.delete<void>(`/api/groups/${groupId}/members/${username}`);
  }
}
