import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { of } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { BranchSwitcher } from './branch-switcher';
import { BranchInfo, MergeRequestsService } from '../../merge-requests/merge-requests.service';
import { ReleasesService, TagSummary } from '../../releases/releases.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';

// `provideRouter`/`provideFerrisgitIcons` return `EnvironmentProviders`, which only fit in an `ApplicationConfig`.
const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

function withRefs(branches: BranchInfo[], tags: TagSummary[]) {
  return moduleMetadata({
    providers: [
      { provide: MergeRequestsService, useValue: { listBranches: () => of(branches) } },
      { provide: ReleasesService, useValue: { listTags: () => of(tags) } },
    ],
  });
}

const FEW_BRANCHES: BranchInfo[] = [
  { name: 'main', tipSha: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678', isDefault: true },
  { name: 'develop', tipSha: 'b2c3d4e5f60718293a4b5c6d7e8f901234567890', isDefault: false },
];

const MANY_BRANCH_NAMES = [
  'feature/paiement-sepa',
  'feature/export-pdf',
  'feature/notifications-email',
  'fix/arrondi-tva',
  'fix/fuseau-horaire',
  'hotfix/certificat-expire',
  'release/2.4',
  'release/2.3',
  'chore/mise-a-jour-angular',
  'refactor/service-facturation',
];

const MANY_BRANCHES: BranchInfo[] = [
  { name: 'main', tipSha: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678', isDefault: true },
  { name: 'develop', tipSha: 'b2c3d4e5f60718293a4b5c6d7e8f901234567890', isDefault: false },
  ...MANY_BRANCH_NAMES.map((name, i) => ({ name, tipSha: `${(i + 3).toString(16).padStart(2, '0')}d4e5f60718293a4b5c6d7e8f9012345678901`, isDefault: false })),
];

const TAGS: TagSummary[] = [
  { name: 'v2.4.0', targetSha: 'c3d4e5f60718293a4b5c6d7e8f90123456789012' },
  { name: 'v2.3.1', targetSha: 'd4e5f60718293a4b5c6d7e8f9012345678901234' },
  { name: 'v2.3.0', targetSha: 'e5f60718293a4b5c6d7e8f901234567890123456' },
];

const meta: Meta<BranchSwitcher> = {
  title: 'Repositories/BranchSwitcher',
  component: BranchSwitcher,
  tags: ['autodocs'],
  args: { repositoryId: 'repo-1', path: ['camille.martin', 'facturation-api'], currentRef: 'HEAD' },
  decorators: [withApp],
};

export default meta;
type Story = StoryObj<BranchSwitcher>;

export const FewBranchesDefaultSelected: Story = {
  decorators: [withRefs(FEW_BRANCHES, [])],
};

export const NonDefaultBranchSelected: Story = {
  args: { currentRef: 'develop' },
  decorators: [withRefs(FEW_BRANCHES, [])],
};

export const ManyBranchesAndTags: Story = {
  decorators: [withRefs(MANY_BRANCHES, TAGS)],
};

export const TagSelected: Story = {
  args: { currentRef: 'v2.3.1' },
  decorators: [withRefs(MANY_BRANCHES, TAGS)],
};
