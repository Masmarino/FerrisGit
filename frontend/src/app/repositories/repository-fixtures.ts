// Plain data builder shared by the repository specs and stories.
import { Repository } from './repositories.service';

/** A private repository owned by the first path segment; override whatever the test cares about. */
export function repositoryFixture(fields: Partial<Repository> = {}): Repository {
  const path = fields.path ?? ['alice', 'hello'];
  return {
    id: 'repo-1',
    name: path.at(-1)!,
    description: '',
    owner: path[0],
    role: 'owner',
    visibility: 'private',
    createdAt: '2026-01-01T00:00:00Z',
    path,
    ...fields,
  };
}
