import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { t } from '../shared/i18n/translator';

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

export type GroupRole = 'reader' | 'contributor' | 'maintainer';

export const GROUP_ROLE_LABELS: Record<GroupRole, string> = {
  get reader() {
    return t('groups.roles.reader');
  },
  get contributor() {
    return t('groups.roles.contributor');
  },
  get maintainer() {
    return t('groups.roles.maintainer');
  },
};

export function isGroupRole(role: string | null): role is GroupRole {
  return role === 'reader' || role === 'contributor' || role === 'maintainer';
}

export interface GroupMembership {
  id: string;
  path: string;
  role: GroupRole;
}

export interface GroupMember {
  userId: string;
  username: string;
  role: GroupRole;
  createdAt: string;
}

/** The message for a refused group or subgroup creation. */
export function groupCreationError(status: number | undefined, what: 'group' | 'subgroup'): string {
  if (status === 400) {
    return t('groups.invalidName');
  }
  if (status === 409) {
    return t('groups.nameTaken');
  }
  return what === 'group' ? t('groups.createFailedGroup') : t('groups.createFailedSubgroup');
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
