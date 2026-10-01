import { Notification } from './notifications.service';
import { notificationLink, notificationQueryParams, notificationSentence } from './notification-display';

function makeNotification(overrides: Partial<Notification>): Notification {
  return {
    id: '1',
    kind: 'issue_assigned',
    repositoryOwner: 'alice',
    repositoryName: 'hello',
    actorUsername: 'bob',
    mergeRequestId: null,
    mergeRequestTitle: null,
    pipelineId: null,
    commitSha: null,
    role: null,
    issueId: null,
    issueNumber: null,
    issueTitle: null,
    read: false,
    createdAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('notificationSentence', () => {
  it('still renders the merge_request_approved sentence (unchanged behavior)', () => {
    const n = makeNotification({ kind: 'merge_request_approved', actorUsername: 'bob', mergeRequestTitle: 'Add feature', mergeRequestId: '1' });
    expect(notificationSentence(n)).toBe('bob a approuvé votre demande de fusion « Add feature »');
  });

  it('renders the issue_assigned sentence with "ticket"', () => {
    const n = makeNotification({ kind: 'issue_assigned', actorUsername: 'bob' });
    expect(notificationSentence(n)).toBe('bob vous a assigné un ticket sur alice/hello');
  });

  it('renders the issue_commented sentence with "ticket"', () => {
    const n = makeNotification({ kind: 'issue_commented', actorUsername: 'bob' });
    expect(notificationSentence(n)).toBe('bob a commenté un ticket sur alice/hello');
  });

  it('renders the issue_closed sentence with "ticket"', () => {
    const n = makeNotification({ kind: 'issue_closed', actorUsername: 'bob', repositoryOwner: 'alice', repositoryName: 'hello' });
    expect(notificationSentence(n)).toBe('bob a fermé un ticket sur alice/hello');
  });

  describe('collaborator roles', () => {
    const added = (role: string | null) => notificationSentence(makeNotification({ kind: 'collaborator_added', actorUsername: 'bob', role }));
    const changed = (role: string | null) => notificationSentence(makeNotification({ kind: 'collaborator_role_changed', actorUsername: 'bob', role }));

    it.each([
      ['reader', 'lecteur'],
      ['contributor', 'contributeur'],
      ['maintainer', 'mainteneur'],
      ['owner', 'propriétaire'],
    ])('shows the role %s as "%s" when added and when changed', (role, label) => {
      expect(added(role)).toBe(`bob vous a ajouté comme ${label} sur alice/hello`);
      expect(changed(role)).toBe(`bob a changé votre rôle en ${label} sur alice/hello`);
    });

    it('falls back to the raw value for a role it does not know', () => {
      expect(added('auditor')).toBe('bob vous a ajouté comme auditor sur alice/hello');
      expect(changed('auditor')).toBe('bob a changé votre rôle en auditor sur alice/hello');
    });

    it('does not read inherited object keys as roles', () => {
      expect(added('constructor')).toBe('bob vous a ajouté comme constructor sur alice/hello');
      expect(added('toString')).toBe('bob vous a ajouté comme toString sur alice/hello');
    });

    it('never prints "null" when the notification carries no role', () => {
      expect(added(null)).toBe('bob vous a ajouté comme collaborateur sur alice/hello');
      expect(changed(null)).toBe('bob a changé votre rôle en collaborateur sur alice/hello');
    });
  });
});

describe('notificationLink', () => {
  it('links to the specific issue when issueNumber is present', () => {
    const n = makeNotification({ kind: 'issue_assigned', issueNumber: 7 });
    expect(notificationLink(n)).toEqual(['/repositories', 'alice', 'hello', '-', 'issues', '7']);
  });

  it('falls back to the repository issues list when issueNumber is absent (an older, pre-migration notification)', () => {
    const n = makeNotification({ kind: 'issue_assigned', issueNumber: null });
    expect(notificationLink(n)).toEqual(['/repositories', 'alice', 'hello', '-', 'issues']);
  });

  it('links to the merge request when mergeRequestId is set (unchanged behavior)', () => {
    const n = makeNotification({ kind: 'merge_request_approved', mergeRequestId: 'mr-1' });
    expect(notificationLink(n)).toEqual(['/repositories', 'alice', 'hello', '-', 'merge-requests', 'mr-1']);
  });

  it('links to the repository root on collaborator_removed (unchanged behavior)', () => {
    const n = makeNotification({ kind: 'collaborator_removed' });
    expect(notificationLink(n)).toEqual(['/repositories']);
  });

  it('falls back to the repository settings link for kinds with no more specific link (unchanged behavior)', () => {
    const n = makeNotification({ kind: 'collaborator_added', role: 'reader' });
    expect(notificationLink(n)).toEqual(['/repositories', 'alice', 'hello', '-', 'settings']);
  });
});

describe('notificationQueryParams', () => {
  it('opens the collaborators section of the settings for a collaborator_added notification', () => {
    const n = makeNotification({ kind: 'collaborator_added', role: 'reader' });
    expect(notificationLink(n)).toEqual(['/repositories', 'alice', 'hello', '-', 'settings']);
    expect(notificationQueryParams(n)).toEqual({ section: 'collaborators' });
  });

  it('opens the collaborators section for a collaborator_role_changed notification too', () => {
    const n = makeNotification({ kind: 'collaborator_role_changed', role: 'maintainer' });
    expect(notificationLink(n)).toEqual(['/repositories', 'alice', 'hello', '-', 'settings']);
    expect(notificationQueryParams(n)).toEqual({ section: 'collaborators' });
  });

  it('returns the same object on every call, so a template binding it never sees a new value', () => {
    const n = makeNotification({ kind: 'collaborator_added', role: 'reader' });
    expect(notificationQueryParams(n)).toBe(notificationQueryParams({ ...n }));
  });

  it('adds no query parameters to any other link', () => {
    expect(notificationQueryParams(makeNotification({ kind: 'issue_assigned', issueNumber: 7 }))).toBeNull();
    expect(notificationQueryParams(makeNotification({ kind: 'merge_request_approved', mergeRequestId: 'mr-1' }))).toBeNull();
    expect(notificationQueryParams(makeNotification({ kind: 'pipeline_failed', pipelineId: 'p1' }))).toBeNull();
    expect(notificationQueryParams(makeNotification({ kind: 'collaborator_removed' }))).toBeNull();
  });
});
