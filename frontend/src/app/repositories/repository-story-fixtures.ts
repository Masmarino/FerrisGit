// Storybook-only fixtures for the repository code pages.
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { HttpErrorResponse } from '@angular/common/http';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { CommitInfo, Contributor, LanguageStat, RepositoriesService, Repository, TreeEntry } from './repositories.service';
import { BranchInfo, MergeRequestsService } from '../merge-requests/merge-requests.service';
import { ReleasesService, TagSummary } from '../releases/releases.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../shared/register-icons';
import { daysAgo, hoursAgo, minutesAgo } from '../shared/layout/page-story-helpers';
import { fakeToast } from '../shared/layout/settings-story-helpers';

// `EnvironmentProviders` only fit in an `ApplicationConfig`, not in `moduleMetadata`'s `Provider[]`.
export const withRouterAndIcons = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

const commit = (sha: string, message: string, authorName: string, committedAt: string): CommitInfo => ({
  sha,
  message,
  authorName,
  authorEmail: `${authorName.toLowerCase().replace(/\s+/g, '.')}@ferrisgit.dev`,
  committedAt,
});

export const REPO: Repository = {
  id: 'repo-1',
  name: 'ferrisgit',
  description: 'Forge Git auto-hébergée écrite en Rust : dépôts, tickets, demandes de fusion, pipelines et wiki dans un seul binaire.',
  owner: 'florian',
  role: 'owner',
  visibility: 'public',
  createdAt: '2025-11-14T09:30:00Z',
  path: ['florian', 'ferrisgit'],
  starCount: 128,
  isStarred: false,
  sizeBytes: 13_004_800,
};

export const PRIVATE_REPO: Repository = {
  ...REPO,
  id: 'repo-2',
  name: 'infra-interne',
  description: 'Scripts Terraform et playbooks Ansible de la plateforme.',
  visibility: 'private',
  path: ['acme', 'plateforme', 'infra-interne'],
  starCount: 3,
  isStarred: true,
  sizeBytes: 812_000,
};

export const EMPTY_REPO: Repository = {
  ...REPO,
  id: 'repo-3',
  name: 'nouveau-projet',
  description: '',
  visibility: 'private',
  createdAt: minutesAgo(12),
  path: ['florian', 'nouveau-projet'],
  starCount: 0,
  sizeBytes: 0,
};

export const COMMITS: CommitInfo[] = [
  commit('9f3c2a17b4e8d6051c2f7a9e3b1d4c6f8a0e2b57', 'Refonte de la page d’accueil du dépôt\n\nEn-tête, bandeau du dernier commit, panneaux latéraux.', 'Florian Simon', minutesAgo(38)),
  commit('4b7e1d9c2a5f8e3b6d0c9a1f4e7b2d5c8a3f6e19', 'Découper le lecteur gix par module', 'Alice Martin', hoursAgo(5)),
  commit('1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b', 'Premier commit', 'Florian Simon', daysAgo(120)),
];

export const ROOT_ENTRIES: TreeEntry[] = [
  { name: 'crates', isDir: true, lastCommit: COMMITS[1] },
  { name: 'frontend', isDir: true, lastCommit: COMMITS[0] },
  { name: 'scripts', isDir: true, lastCommit: commit('c0ffee00', 'Ajouter le script de démarrage local', 'Bastien Petit', daysAgo(6)) },
  { name: '.github', isDir: true, lastCommit: commit('d00dfeed', 'Lancer les tests frontend en CI', 'Alice Martin', daysAgo(21)) },
  { name: 'Cargo.toml', isDir: false, lastCommit: commit('beefcafe', 'Passer à gix 0.72', 'Florian Simon', daysAgo(3)) },
  { name: 'Cargo.lock', isDir: false, lastCommit: commit('beefcafe', 'Passer à gix 0.72', 'Florian Simon', daysAgo(3)) },
  { name: 'README.md', isDir: false, lastCommit: commit('facade00', 'Documenter les variables d’environnement', 'Chloé Fontaine', daysAgo(12)) },
  { name: 'LICENSE', isDir: false, lastCommit: commit('1a2b3c4d', 'Premier commit', 'Florian Simon', daysAgo(120)) },
];

export const FOLDER_ENTRIES: TreeEntry[] = [
  { name: 'ferrisgit-api', isDir: true, lastCommit: COMMITS[1] },
  { name: 'ferrisgit-domain', isDir: true, lastCommit: commit('abad1dea', 'Ajouter les jalons aux tickets', 'Alice Martin', daysAgo(2)) },
  { name: 'ferrisgit-infrastructure', isDir: true, lastCommit: COMMITS[1] },
  { name: 'README.md', isDir: false, lastCommit: commit('0ddba11', 'Décrire l’architecture hexagonale', 'Florian Simon', daysAgo(40)) },
];

export const LONG_NAME_ENTRIES: TreeEntry[] = [
  { name: 'un-dossier-au-nom-vraiment-très-long-pour-tester-la-troncature-des-noms', isDir: true, lastCommit: commit('5eed5eed', 'Un message de commit lui aussi particulièrement long, qui décrit en détail tout ce qui a changé et pourquoi', 'Maximilien de La Tour d’Auvergne', hoursAgo(2)) },
  { name: 'src', isDir: true, lastCommit: COMMITS[0] },
  { name: 'fichier-de-configuration-de-l-environnement-de-developpement-local.example.toml', isDir: false, lastCommit: commit('5eed5eed', 'Un message de commit lui aussi particulièrement long, qui décrit en détail tout ce qui a changé et pourquoi', 'Maximilien de La Tour d’Auvergne', hoursAgo(2)) },
  { name: 'README.md', isDir: false, lastCommit: null },
];

export const README = `# FerrisGit

FerrisGit est une forge Git **auto-hébergée** écrite en Rust : dépôts, tickets, demandes de fusion,
pipelines et wiki dans un seul binaire.

## Démarrer en local

1. Lancez la base de données : \`docker compose up -d postgres\`
2. Démarrez l’API et l’interface : \`scripts/dev.sh\`
3. Ouvrez [l’interface](http://localhost:4201) et créez le premier compte administrateur.

\`\`\`bash
git clone https://git.ferrisgit.dev/florian/ferrisgit.git
cd ferrisgit && scripts/dev.sh
\`\`\`

> Les migrations de la base sont appliquées automatiquement au démarrage.

## Configuration

| Variable | Rôle | Défaut |
| --- | --- | --- |
| \`DATABASE_URL\` | Connexion PostgreSQL | — |
| \`FERRISGIT_DATA_DIR\` | Dossier des dépôts | \`./data\` |
| \`FERRISGIT_PORT\` | Port HTTP | \`8080\` |

## Contribuer

Ouvrez une demande de fusion vers \`main\` en décrivant le changement et la façon de le tester.
`;

export const LONG_README = `${README}
## Architecture

Le code suit une architecture hexagonale : le domaine ne dépend d’aucune bibliothèque d’accès aux
données, les adaptateurs (PostgreSQL, gix, Kubernetes) implémentent ses ports.

### Crates

- \`ferrisgit-domain\` : entités, règles métier et ports ;
- \`ferrisgit-application\` : cas d’usage ;
- \`ferrisgit-infrastructure\` : adaptateurs PostgreSQL, gix et Kubernetes ;
- \`ferrisgit-api\` : routes HTTP (axum) et protocole Git smart HTTP.

### Frontend

L’interface est une application Angular sans zone.js, construite sur le système de design Gabarit.
Chaque composant a ses tests unitaires et ses stories Storybook, vérifiées en thème clair et sombre.

---

## Licence

Distribué sous licence AGPL-3.0. Voir \`LICENSE\` pour le texte complet, qui est volontairement reproduit ici sur une ligne très longue afin de vérifier que le texte se replie correctement dans la carte sans provoquer de défilement horizontal.
`;

export const CONTRIBUTORS: Contributor[] = [
  { name: 'Florian Simon', email: 'florian@ferrisgit.dev', commitCount: 412 },
  { name: 'Alice Martin', email: 'alice@ferrisgit.dev', commitCount: 187 },
  { name: 'Bastien Petit', email: 'bastien@ferrisgit.dev', commitCount: 64 },
  { name: 'Chloé Fontaine', email: 'chloe@ferrisgit.dev', commitCount: 23 },
  { name: 'Maximilien de La Tour d’Auvergne', email: 'max@ferrisgit.dev', commitCount: 1 },
];

export const LANGUAGES: LanguageStat[] = [
  { name: 'Rust', bytes: 812_000, percentage: 61.8 },
  { name: 'TypeScript', bytes: 322_000, percentage: 24.5 },
  { name: 'HTML', bytes: 82_000, percentage: 6.2 },
  { name: 'CSS', bytes: 61_000, percentage: 4.6 },
  { name: 'Shell', bytes: 25_000, percentage: 1.9 },
  { name: 'Other', bytes: 13_000, percentage: 1 },
];

export const BRANCHES: BranchInfo[] = [
  { name: 'main', tipSha: COMMITS[0].sha, isDefault: true },
  { name: 'develop', tipSha: COMMITS[1].sha, isDefault: false },
  { name: 'feature/refonte-des-pages-du-depot', tipSha: COMMITS[1].sha, isDefault: false },
];

export const TAGS: TagSummary[] = [{ name: 'v1.4.0', targetSha: COMMITS[2].sha }];

export interface RepositoryFixture {
  repo: Repository;
  entries: TreeEntry[];
  folderEntries: TreeEntry[];
  readme: string | null;
  commits: CommitInfo[];
  contributors: Contributor[];
  languages: LanguageStat[];
  /** Tree listing error (404 = empty repository at HEAD, or unknown ref/path elsewhere). */
  treeStatus?: number;
}

export const POPULATED: RepositoryFixture = {
  repo: REPO,
  entries: ROOT_ENTRIES,
  folderEntries: FOLDER_ENTRIES,
  readme: README,
  commits: COMMITS,
  contributors: CONTRIBUTORS,
  languages: LANGUAGES,
};

export function fakeRepositoriesService(fixture: RepositoryFixture): Partial<RepositoriesService> {
  const tree = (path: string[]): Observable<TreeEntry[]> => {
    if (fixture.treeStatus) {
      return throwError(() => new HttpErrorResponse({ status: fixture.treeStatus, statusText: 'Not Found' }));
    }
    return of(path.length === 0 ? fixture.entries : fixture.folderEntries);
  };
  let starCount = fixture.repo.starCount ?? 0;
  return {
    getById: () => of(fixture.repo),
    treeAt: (_id: string, _ref: string, path: string[]) => tree(path),
    readmeAt: () => of({ content: fixture.readme }),
    commitsById: () => of(fixture.commits),
    listContributors: () => of(fixture.contributors),
    getLanguages: () => of({ languages: fixture.languages }),
    star: () => of({ starCount: ++starCount, isStarred: true }),
    unstar: () => of({ starCount: --starCount, isStarred: false }),
    cloneUrl: (path: string[]) => `https://git.ferrisgit.dev/${path.join('/')}.git`,
  };
}

export const LOADING_REPOSITORIES: Partial<RepositoriesService> = {
  getById: () => NEVER,
  treeAt: () => NEVER,
  readmeAt: () => NEVER,
  commitsById: () => NEVER,
  listContributors: () => NEVER,
  getLanguages: () => NEVER,
  cloneUrl: () => '',
};

export function withRepository(repositories: Partial<RepositoriesService>, refs: { branches: BranchInfo[]; tags: TagSummary[] } = { branches: BRANCHES, tags: TAGS }) {
  return moduleMetadata({
    providers: [
      { provide: RepositoriesService, useValue: repositories },
      { provide: MergeRequestsService, useValue: { listBranches: () => of(refs.branches) } },
      { provide: ReleasesService, useValue: { listTags: () => of(refs.tags) } },
      { provide: GbtToastService, useValue: fakeToast },
    ],
  });
}
