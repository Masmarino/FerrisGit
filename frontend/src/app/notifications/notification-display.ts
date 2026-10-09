import { Notification } from './notifications.service';
import { t } from '../shared/i18n/translator';

const ROLES = ['reader', 'contributor', 'maintainer', 'owner'];

function roleLabel(role: string | null): string {
  if (role === null) {
    return t('notifications.roles.unknown');
  }
  return ROLES.includes(role) ? t(`notifications.roles.${role}`) : role;
}

export function notificationSentence(n: Notification): string {
  const actor = n.actorUsername;
  const title = n.mergeRequestTitle;
  const repository = `${n.repositoryOwner}/${n.repositoryName}`;
  switch (n.kind) {
    case 'merge_request_approved':
      return t('notifications.kinds.merge_request_approved', { actor, title });
    case 'merge_request_changes_requested':
      return t('notifications.kinds.merge_request_changes_requested', { actor, title });
    case 'merge_request_commented':
      return t('notifications.kinds.merge_request_commented', { actor, title });
    case 'merge_request_merged':
      return t('notifications.kinds.merge_request_merged', { actor, title });
    case 'merge_request_closed':
      return t('notifications.kinds.merge_request_closed', { actor, title });
    case 'collaborator_added':
      return t('notifications.kinds.collaborator_added', { actor, role: roleLabel(n.role), repository });
    case 'collaborator_role_changed':
      return t('notifications.kinds.collaborator_role_changed', { actor, role: roleLabel(n.role), repository });
    case 'collaborator_removed':
      return t('notifications.kinds.collaborator_removed', { actor, repository });
    case 'pipeline_failed':
      return t('notifications.kinds.pipeline_failed', { commit: n.commitSha?.slice(0, 8), repository });
    case 'issue_assigned':
      return t('notifications.kinds.issue_assigned', { actor, repository });
    case 'issue_commented':
      return t('notifications.kinds.issue_commented', { actor, repository });
    case 'issue_closed':
      return t('notifications.kinds.issue_closed', { actor, repository });
    default:
      return t('notifications.kinds.unknown');
  }
}

// Known limitation: a notification only stores the repository's creator and name, so for a group repository this link
// can land on an unrelated personal repository of the creator with the same name. A real fix needs a repository id on
// the notification.
export function notificationLink(n: Notification): string[] {
  const repository = ['/repositories', n.repositoryOwner, n.repositoryName, '-'];
  if (n.mergeRequestId) {
    return [...repository, 'merge-requests', n.mergeRequestId];
  }
  if (n.pipelineId) {
    return [...repository, 'pipelines', n.pipelineId];
  }
  if (n.kind === 'issue_assigned' || n.kind === 'issue_commented' || n.kind === 'issue_closed') {
    return n.issueNumber !== null ? [...repository, 'issues', String(n.issueNumber)] : [...repository, 'issues'];
  }
  return n.kind === 'collaborator_removed' ? ['/repositories'] : [...repository, 'settings'];
}

const COLLABORATORS_SECTION: Readonly<Record<string, string>> = Object.freeze({ section: 'collaborators' });

/** Always the same object, so a template can bind it without giving the router a new value on every change detection. */
export function notificationQueryParams(n: Notification): Readonly<Record<string, string>> | null {
  if (n.mergeRequestId || n.pipelineId) {
    return null;
  }
  return n.kind === 'collaborator_added' || n.kind === 'collaborator_role_changed' ? COLLABORATORS_SECTION : null;
}
