import { Notification } from './notifications.service';

const ROLE_LABELS: ReadonlyMap<string, string> = new Map([
  ['reader', 'lecteur'],
  ['contributor', 'contributeur'],
  ['maintainer', 'mainteneur'],
  ['owner', 'propriétaire'],
]);

function roleLabel(role: string | null): string {
  return role === null ? 'collaborateur' : (ROLE_LABELS.get(role) ?? role);
}

export function notificationSentence(n: Notification): string {
  switch (n.kind) {
    case 'merge_request_approved':
      return `${n.actorUsername} a approuvé votre demande de fusion « ${n.mergeRequestTitle} »`;
    case 'merge_request_changes_requested':
      return `${n.actorUsername} a demandé des changements sur votre demande de fusion « ${n.mergeRequestTitle} »`;
    case 'merge_request_commented':
      return `${n.actorUsername} a commenté votre demande de fusion « ${n.mergeRequestTitle} »`;
    case 'merge_request_merged':
      return `${n.actorUsername} a fusionné votre demande de fusion « ${n.mergeRequestTitle} »`;
    case 'merge_request_closed':
      return `${n.actorUsername} a fermé votre demande de fusion « ${n.mergeRequestTitle} »`;
    case 'collaborator_added':
      return `${n.actorUsername} vous a ajouté comme ${roleLabel(n.role)} sur ${n.repositoryOwner}/${n.repositoryName}`;
    case 'collaborator_role_changed':
      return `${n.actorUsername} a changé votre rôle en ${roleLabel(n.role)} sur ${n.repositoryOwner}/${n.repositoryName}`;
    case 'collaborator_removed':
      return `${n.actorUsername} vous a retiré de ${n.repositoryOwner}/${n.repositoryName}`;
    case 'pipeline_failed':
      return `Le pipeline sur ${n.commitSha?.slice(0, 8)} a échoué (${n.repositoryOwner}/${n.repositoryName})`;
    case 'issue_assigned':
      return `${n.actorUsername} vous a assigné un ticket sur ${n.repositoryOwner}/${n.repositoryName}`;
    case 'issue_commented':
      return `${n.actorUsername} a commenté un ticket sur ${n.repositoryOwner}/${n.repositoryName}`;
    case 'issue_closed':
      return `${n.actorUsername} a fermé un ticket sur ${n.repositoryOwner}/${n.repositoryName}`;
    default:
      return 'Nouvelle notification';
  }
}

// Known limitation: a notification only stores the repository's creator and name. For a group repository, this link
// can point to an unrelated personal repository of the creator with the same name. A real fix needs a repository id on
// the notification. The `-` marker only makes personal-repository links correct.
export function notificationLink(n: Notification): string[] {
  if (n.mergeRequestId) {
    return ['/repositories', n.repositoryOwner, n.repositoryName, '-', 'merge-requests', n.mergeRequestId];
  } else if (n.pipelineId) {
    return ['/repositories', n.repositoryOwner, n.repositoryName, '-', 'pipelines', n.pipelineId];
  } else if (n.kind === 'issue_assigned' || n.kind === 'issue_commented' || n.kind === 'issue_closed') {
    if (n.issueNumber !== null) {
      return ['/repositories', n.repositoryOwner, n.repositoryName, '-', 'issues', String(n.issueNumber)];
    }
    return ['/repositories', n.repositoryOwner, n.repositoryName, '-', 'issues'];
  } else if (n.kind === 'collaborator_removed') {
    return ['/repositories'];
  } else {
    return ['/repositories', n.repositoryOwner, n.repositoryName, '-', 'settings'];
  }
}

const COLLABORATORS_SECTION: Readonly<Record<string, string>> = Object.freeze({ section: 'collaborators' });

/** Always the same object, so a template can bind it without handing the router a new value on every change detection. */
export function notificationQueryParams(n: Notification): Readonly<Record<string, string>> | null {
  if (n.mergeRequestId || n.pipelineId) {
    return null;
  }
  return n.kind === 'collaborator_added' || n.kind === 'collaborator_role_changed' ? COLLABORATORS_SECTION : null;
}
