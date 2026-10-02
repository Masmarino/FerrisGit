import { Repository, TreeEntry } from './repositories.service';

export interface BreadcrumbSegment {
  name: string;
  link: string[];
}

export function repositoryLink(repositoryPath: string[], kind: 'tree' | 'blob', ref: string, path: string[] = []): string[] {
  return ['/repositories', ...repositoryPath, '-', kind, ref, ...path];
}

/** Repository name, then the folders above the last segment (that one is the current page, so no link). */
export function pathBreadcrumb(repo: Repository | null, ref: string, path: string[]): BreadcrumbSegment[] {
  if (!repo) {
    return [];
  }
  const root = repositoryLink(repo.path, 'tree', ref);
  return [{ name: repo.name, link: root }, ...path.slice(0, -1).map((name, i) => ({ name, link: [...root, ...path.slice(0, i + 1)] }))];
}

export function sortEntries(entries: TreeEntry[]): TreeEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}
