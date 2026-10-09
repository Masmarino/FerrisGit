import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { signal } from '@angular/core';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { HttpErrorResponse } from '@angular/common/http';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { PipelineEditor } from './pipeline-editor';
import { DefinitionDto, ParsedPipeline, PipelineDefinitionsService, PipelineProposal, RenderedPipeline, RepositoryPipelineFile, RepositoryProfile } from './pipeline-definitions.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { RepositorySettingsService } from '../../repositories/repository-settings.service';
import { SettingsService } from '../../settings/settings.service';
import { fakeRepositoryContextService } from '../../issues/issue-story-fixtures';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { atPhoneWidth, inShellContentArea } from '../../shared/layout/page-story-helpers';

const job = (stage: string, image: string, script: string[], extra: Partial<DefinitionDto['jobs'][string]> = {}) => ({ stage, image, script, variables: {}, needs: [], tags: [], cache: [], ...extra });

const DEFINITION: DefinitionDto = {
  stages: ['build', 'test', 'publish'],
  jobs: {
    compile: job('build', 'rust:1', ['cargo build --release'], { cache: ['cargo-registry'] }),
    frontend: job('build', 'node:22', ['npm ci', 'npm run build'], { cache: ['npm'] }),
    lint: job('test', 'rust:1', ['cargo fmt --all -- --check', 'cargo clippy -- -D warnings'], { needs: ['compile'] }),
    'unit-tests': job('test', 'rust:1', ['cargo test'], { needs: ['compile'], variables: { RUST_LOG: 'debug' } }),
    release: job('publish', 'docker:27', ['docker build -t app .', 'docker push app'], { needs: ['unit-tests', 'frontend'], tags: ['docker'] }),
  },
};

/** A readable stand-in for the server's YAML, so that the story shows a realistic file. */
function yamlOf(definition: DefinitionDto): string {
  const jobs = Object.entries(definition.jobs)
    .map(([name, j]) => `  ${name}:\n    stage: ${j.stage}\n    image: ${j.image}\n    script:\n${j.script.map((line) => `    - ${line}`).join('\n')}${j.needs.length ? `\n    needs:\n${j.needs.map((n) => `    - ${n}`).join('\n')}` : ''}`)
    .join('\n');
  return `stages:\n${definition.stages.map((s) => `- ${s}`).join('\n')}\njobs:\n${jobs}\n`;
}

const noProblems = (definition: DefinitionDto): RenderedPipeline => ({ yaml: yamlOf(definition), problems: [], warnings: [] });

const PARSED: ParsedPipeline = { definition: DEFINITION, problems: [], warnings: [], ignoredFields: [], hasComments: false };

const FILE: RepositoryPipelineFile = { path: '.ferrisgit-ci.yml', branch: 'main', baseSha: '3f9a1c2b7d0e4a5f8b6c1d2e3f4a5b6c7d8e9f01', yaml: 'stages: [build]' };

interface Options {
  role?: 'owner' | 'maintainer' | 'contributor' | 'reader';
  /** The repository's secrets by name, as a maintainer sees them. */
  secrets?: string[];
  engine?: 'docker-runners' | 'kubernetes';
  file?: Observable<RepositoryPipelineFile>;
  parsed?: ParsedPipeline;
  /** How typed YAML is reported. By default, it parses fine. */
  parse?: (yaml: string) => Observable<ParsedPipeline>;
  render?: (definition: DefinitionDto) => Observable<RenderedPipeline>;
  propose?: () => Observable<PipelineProposal>;
  /** What the repository is made of. By default, nothing the editor recognises. */
  profile?: Observable<RepositoryProfile>;
}

/** This repository's own layout: a Rust workspace with offline queries, two Angular apps, an image and a chart. */
const FERRISGIT_PROFILE: RepositoryProfile = {
  projects: [
    { kind: 'rust', dir: '', evidence: ['Cargo.toml', 'rust-toolchain.toml'], workspace: true, toolchain: '1.98.1', sqlxOffline: true, sqlxPostgres: true },
    { kind: 'node', dir: 'frontend', evidence: ['frontend/package.json', 'frontend/package-lock.json', 'frontend/angular.json'], packageManager: 'npm', nodeVersion: '26', scripts: { build: 'ng build', test: 'ng test' }, framework: 'angular', testRunner: 'vitest' },
    { kind: 'node', dir: 'website', evidence: ['website/package.json', 'website/package-lock.json'], packageManager: 'npm', nodeVersion: '26', scripts: { build: 'ng build', test: 'ng test --watch=false', lint: 'eslint .' }, framework: 'angular', testRunner: 'vitest' },
  ],
  dockerfiles: [''],
  helmCharts: ['helm/ferrisgit'],
};

function withData(options: Options = {}) {
  return moduleMetadata({
    providers: [
      {
        provide: PipelineDefinitionsService,
        useValue: {
          repositoryFile: () => options.file ?? of(FILE),
          parse: options.parse ?? (() => of(options.parsed ?? PARSED)),
          render: options.render ?? ((definition: DefinitionDto) => of(noProblems(definition))),
          propose: options.propose ?? (() => of({ branch: 'pipeline-editor/3f9a1c2b', commitSha: 'c1', mergeRequestId: 'mr-1' })),
          repositoryProfile: () => options.profile ?? of({ projects: [], dockerfiles: [], helmCharts: [] }),
        },
      },
      { provide: RepositoryContextService, useValue: fakeRepositoryContextService(options.role ?? 'owner') },
      {
        provide: RepositorySettingsService,
        useValue: {
          listCiVariables: () => of((options.secrets ?? ['REGISTRY_USER']).map((key, index) => ({ id: `v${index}`, key, masked: true }))),
          setCiVariable: (_id: string, key: string) => of({ id: 'new', key, masked: true }),
          deleteCiVariable: () => of(undefined),
        },
      },
      { provide: SettingsService, useValue: { publicSettings: signal({ executionEngine: options.engine ?? 'docker-runners' }), loadPublic: () => undefined } },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();

/**
 * Layout checks jsdom cannot do: the page does not scroll sideways, the lanes are level and inside the board, and each
 * card stays inside its lane.
 */
function assertBoardLayout(canvas: HTMLElement): void {
  const board = canvas.querySelector('.pipeline-editor__board');
  const lanes = Array.from(canvas.querySelectorAll('.pipeline-editor__lane:not(.pipeline-editor__lane--new)'));
  if (!board || lanes.length === 0) throw new Error('board not rendered yet');
  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`page overflows sideways: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);
  for (const [index, lane] of lanes.entries()) {
    if (rect(lane).width < 259.5) throw new Error(`stage ${index} is narrower than 260px`);
    if (Math.abs(rect(lane).top - rect(lanes[0]).top) > 0.5) throw new Error('stages are not level');
    for (const card of Array.from(lane.querySelectorAll('.pipeline-editor__card'))) {
      if (rect(card).left < rect(lane).left || rect(card).right > rect(lane).right + 0.5) throw new Error('card outside its stage');
      const name = card.querySelector('.pipeline-editor__card-name')!;
      if (rect(name).right > rect(card).right) throw new Error('job name spills out of its card');
    }
  }
}

async function expectBoardLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertBoardLayout(canvasElement), { timeout: 3000 });
}

const meta: Meta<PipelineEditor> = {
  title: 'Pipelines/PipelineEditor',
  component: PipelineEditor,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', path: ['alice', 'ferrisgit'] },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<PipelineEditor>;

export const Populated: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectBoardLayout(context);
    await waitFor(() => expect(context.canvasElement.querySelector('.pipeline-editor__yaml')?.textContent).toContain('unit-tests'));
    await expect(context.canvasElement.querySelectorAll('.pipeline-editor__card')).toHaveLength(5);
  },
};

export const NoFileYet: Story = {
  decorators: [withData({ file: of({ ...FILE, yaml: null }) })],
  play: async (context) => {
    await waitFor(() => expect(context.canvasElement.querySelector('.pipeline-editor__notes')?.textContent).toContain("pas encore de fichier"));
    await expectBoardLayout(context);
  },
};

export const WithProblems: Story = {
  decorators: [
    withData({
      render: (definition) =>
        of({
          yaml: yamlOf(definition),
          problems: [
            { code: 'unknown_dependency', message: 'x', job: 'lint', dependency: 'compil' },
            { code: 'invalid_cache_key', message: 'x', job: 'frontend', key: 'NPM Cache' },
          ],
          warnings: [{ code: 'empty_script', job: 'release' }],
        }),
    }),
  ],
  play: async (context) => {
    await expectBoardLayout(context);
    await waitFor(() => expect(context.canvasElement.querySelectorAll('.pipeline-editor__card--problem')).toHaveLength(2));
    await expect(context.canvasElement.querySelectorAll('.pipeline-editor__issue-link').length).toBe(3);
  },
};

export const WhatARewriteLoses: Story = {
  decorators: [withData({ parsed: { ...PARSED, hasComments: true, ignoredFields: ['include', 'jobs.release.when'] } })],
  play: async (context) => {
    await waitFor(() => expect(context.canvasElement.querySelector('.pipeline-editor__notes')?.textContent).toContain('commentaires'));
    await expectBoardLayout(context);
  },
};

export const JobOpen: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('[data-job="unit-tests"] .pipeline-editor__card-open')!.click();
    await waitFor(() => expect(context.canvasElement.ownerDocument.querySelector('fg-pipeline-job-form')).not.toBeNull());
  },
};

export const MoveMenuOpen: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('[data-job="lint"] .pipeline-editor__actions .gbt-menu__trigger')!.click();
    await waitFor(() =>
      expect(Array.from(context.canvasElement.querySelectorAll('[role="menuitem"]'), (item) => item.textContent?.trim())).toEqual(['Modifier', 'Dupliquer', 'Déplacer vers build', 'Déplacer vers publish', 'Supprimer']),
    );
  },
};

export const ReadOnly: Story = {
  decorators: [withData({ role: 'reader' })],
  play: async (context) => {
    await waitFor(() => expect(context.canvasElement.textContent).toContain('Réservé aux contributeurs'));
    await expect(context.canvasElement.querySelector('.pipeline-editor__board')).toBeNull();
  },
};

export const Loading: Story = {
  decorators: [withData({ file: NEVER })],
};

export const LoadError: Story = {
  decorators: [withData({ file: throwError(() => new HttpErrorResponse({ status: 500 })) })],
};

export const PhoneWidth: Story = {
  decorators: [withData(), atPhoneWidth],
  play: async ({ canvasElement }) => {
    await waitFor(() => {
      const doc = canvasElement.ownerDocument.documentElement;
      if (!canvasElement.querySelector('.pipeline-editor__board')) throw new Error('not rendered yet');
      if (doc.scrollWidth > doc.clientWidth + 1) throw new Error('page overflows sideways');
    });
  },
};

/** The same file, typed: what gets saved is this text, comments included. */
export const YamlMode: Story = {
  decorators: [withData({ file: of({ ...FILE, yaml: '# Build and test\nstages: [build, test]\njobs:\n  compile:\n    stage: build\n    image: rust:1\n    script: [cargo build]\n' }) })],
  play: async (context) => {
    context.canvasElement.querySelector<HTMLButtonElement>('gbt-segmented-control button:last-child')?.click();
    await waitFor(() => expect(context.canvasElement.querySelector('.pipeline-editor__yaml-input textarea')).not.toBeNull());
    await expect(context.canvasElement.querySelector('.pipeline-editor__board')).toBeNull();
    await waitFor(() => expect(context.canvasElement.querySelector<HTMLTextAreaElement>('.pipeline-editor__yaml-input textarea')!.value).toContain('# Build and test'));
  },
};

/** Typed YAML the server cannot parse: the cards wait until it can. */
export const YamlUnreadable: Story = {
  decorators: [withData({ parse: (yaml) => of(yaml.includes('[') && !yaml.includes(']') ? { ...PARSED, definition: null, problems: [{ code: 'invalid_yaml', message: 'invalid YAML: did not find expected node at line 2 column 1' }] } : PARSED) })],
  play: async (context) => {
    context.canvasElement.querySelector<HTMLButtonElement>('gbt-segmented-control button:last-child')?.click();
    const area = await waitFor(() => {
      const el = context.canvasElement.querySelector<HTMLTextAreaElement>('.pipeline-editor__yaml-input textarea');
      if (!el) throw new Error('not rendered yet');
      return el;
    });
    area.value = 'jobs: [';
    area.dispatchEvent(new Event('input', { bubbles: true }));
    await waitFor(() => expect(context.canvasElement.querySelector('.pipeline-editor__issues')?.textContent).toContain("n'est pas un YAML valide"));
  },
};

/** A change, then the dialog that turns it into a branch and a merge request. */
export const SaveDialog: Story = {
  decorators: [withData()],
  play: async (context) => {
    const doc = context.canvasElement.ownerDocument;
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('.pipeline-editor__add-job button')!.click();
    const empty = await waitFor(() => {
      const button = doc.querySelector<HTMLButtonElement>('[data-tile="custom"] .tile-picker__choose');
      if (!button) throw new Error('tiles not shown yet');
      return button;
    });
    empty.click();
    await waitFor(() => expect(doc.querySelector('fg-pipeline-job-form')).not.toBeNull());
    doc.querySelector<HTMLButtonElement>('gbt-drawer button[aria-label="Fermer"]')!.click();
    const save = await waitFor(() => {
      const button = Array.from(doc.querySelectorAll<HTMLButtonElement>('gbt-page-header button')).find((b) => b.textContent?.trim() === 'Proposer la modification');
      if (!button || button.disabled) throw new Error('save not offered yet');
      return button;
    });
    save.click();
    await waitFor(() => expect(doc.querySelector('gbt-modal .pipeline-editor__save')).not.toBeNull());
  },
};

export const NoCommitYet: Story = {
  decorators: [withData({ file: of({ path: '.ferrisgit-ci.yml', branch: null, baseSha: null, yaml: null }) })],
  play: async (context) => {
    await waitFor(() => expect(context.canvasElement.querySelector('.pipeline-editor__notes')?.textContent).toContain('aucun commit'));
    await expectBoardLayout(context);
  },
};

/** An empty pipeline: whole templates to start from. */
export const Templates: Story = {
  decorators: [withData({ file: of({ ...FILE, yaml: null }) })],
  play: async (context) => {
    await waitFor(() => expect(context.canvasElement.querySelectorAll('.starters__card').length).toBeGreaterThan(2));
    await expectBoardLayout(context);
  },
};

/**
 * An empty pipeline in a repository the editor recognises: the pipeline proposed for it comes first, with the reasons.
 */
export const ProposedForTheRepository: Story = {
  decorators: [withData({ file: of({ ...FILE, yaml: null }), profile: of(FERRISGIT_PROFILE) })],
  play: async (context) => {
    await waitFor(() => expect(context.canvasElement.querySelector('[data-prediction] h2')?.textContent?.trim()).toBe('Pour ce dépôt : Rust et Angular'));
    await expectBoardLayout(context);
  },
};

/** The same at phone width: the reasons stack under their files. */
export const ProposedAtPhoneWidth: Story = {
  decorators: [withData({ file: of({ ...FILE, yaml: null }), profile: of(FERRISGIT_PROFILE) }), atPhoneWidth],
  play: async (context) => {
    const proposal = await waitFor(() => {
      const found = context.canvasElement.querySelector<HTMLElement>('[data-prediction]');
      if (!found) throw new Error('no proposal yet');
      return found;
    });
    await expect(proposal.scrollWidth).toBeLessThanOrEqual(proposal.clientWidth + 1);
  },
};

/** While the repository is being read. */
export const ReadingTheRepository: Story = {
  decorators: [withData({ file: of({ ...FILE, yaml: null }), profile: NEVER })],
  play: async (context) => {
    await waitFor(() => expect(context.canvasElement.querySelector('fg-pipeline-starters [role="status"]')?.textContent).toBe('Lecture du dépôt'));
  },
};

/** "Ajouter un job" in a repository the editor recognises: the jobs its projects call for come first. */
export const TilePickerForTheRepository: Story = {
  decorators: [withData({ profile: of(FERRISGIT_PROFILE) })],
  play: async (context) => {
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('.pipeline-editor__add-job button')!.click();
    await waitFor(() => expect(context.canvasElement.ownerDocument.querySelectorAll('[data-suggestion]').length).toBeGreaterThan(3));
  },
};

/** "Ajouter un job": the tiles, grouped by purpose. */
export const TilePicker: Story = {
  decorators: [withData({ secrets: ['REGISTRY_USER'] })],
  play: async (context) => {
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('.pipeline-editor__add-job button')!.click();
    await waitFor(() => expect(context.canvasElement.ownerDocument.querySelectorAll('.tile-picker__tile').length).toBeGreaterThan(8));
  },
};

/** A maintainer's panel: what the jobs read and lack, what exists, and the form. */
export const SecretsPanel: Story = {
  decorators: [
    withData({
      secrets: ['API_KEY', 'UNUSED'],
      parsed: {
        ...PARSED,
        definition: { ...DEFINITION, jobs: { ...DEFINITION.jobs, release: { ...DEFINITION.jobs['release'], script: ['curl -H "Authorization: Bearer $DEPLOY_TOKEN" "$API_KEY"'] } } },
      },
    }),
  ],
  play: async (context) => {
    await expectBoardLayout(context);
    const open = Array.from(context.canvasElement.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === 'Variables et secrets')!;
    open.click();
    await waitFor(() => expect(context.canvasElement.ownerDocument.querySelector('fg-pipeline-secrets')).not.toBeNull());
  },
};

export const SecretsPanelForAContributor: Story = {
  decorators: [withData({ role: 'contributor' })],
  play: async (context) => {
    await expectBoardLayout(context);
    Array.from(context.canvasElement.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === 'Variables et secrets')!.click();
    await waitFor(() => expect(context.canvasElement.ownerDocument.querySelector('fg-pipeline-secrets')?.textContent).toContain('Seul un mainteneur'));
  },
};

/** A job that reads a secret nobody created, and has a variable that is a secret in disguise. */
export const JobWithSecrets: Story = {
  decorators: [
    withData({
      secrets: ['API_KEY'],
      parsed: {
        ...PARSED,
        definition: {
          ...DEFINITION,
          jobs: {
            ...DEFINITION.jobs,
            release: { ...DEFINITION.jobs['release'], script: ['docker login -p "$REGISTRY_PASSWORD"', 'curl "$API_KEY" "$WHO"'], variables: { DEPLOY_TOKEN: 'abc', MODE: 'fast' } },
          },
        },
      },
    }),
  ],
  play: async (context) => {
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('[data-job="release"] .pipeline-editor__card-open')!.click();
    await waitFor(() => expect(context.canvasElement.ownerDocument.querySelector('[data-secret-warning]')).not.toBeNull());
    await expect(context.canvasElement.ownerDocument.querySelector('[data-note="unknown"]')?.textContent).toContain('$REGISTRY_PASSWORD');
  },
};

/** The "?" bubble of a stage, opened. */
export const HelpBubble: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('.pipeline-editor__lane-label fg-help-tip button')!.click();
    await waitFor(() => expect(context.canvasElement.ownerDocument.querySelector('.help-tip__panel')?.textContent).toContain('Un palier de la pipeline'));
  },
};

/** A tile with questions: the form, with the commands it will run. */
export const DeployToAVm: Story = {
  decorators: [withData({ secrets: ['SSH_PRIVATE_KEY'] })],
  play: async (context) => {
    const doc = context.canvasElement.ownerDocument;
    await expectBoardLayout(context);
    Array.from(context.canvasElement.querySelectorAll<HTMLButtonElement>('.pipeline-editor__add-job button')).at(-1)!.click();
    const tile = await waitFor(() => {
      const el = doc.querySelector<HTMLButtonElement>('[data-tile="ssh-run"] .tile-picker__choose');
      if (!el) throw new Error('catalogue not open yet');
      return el;
    });
    tile.click();
    await waitFor(() => expect(doc.querySelector('.tile-form__preview')?.textContent).toContain('deploy@vm.example.com'));
  },
};

export const DeployToKubernetes: Story = {
  decorators: [withData({ secrets: [] })],
  play: async (context) => {
    const doc = context.canvasElement.ownerDocument;
    await expectBoardLayout(context);
    Array.from(context.canvasElement.querySelectorAll<HTMLButtonElement>('.pipeline-editor__add-job button')).at(-1)!.click();
    const tile = await waitFor(() => {
      const el = doc.querySelector<HTMLButtonElement>('[data-tile="k8s-image"] .tile-picker__choose');
      if (!el) throw new Error('catalogue not open yet');
      return el;
    });
    tile.click();
    await waitFor(() => expect(doc.querySelector('.tile-form__preview')?.textContent).toContain('kubectl set image'));
    await expect(doc.querySelector('[data-secret="KUBE_CONFIG"]')?.textContent).toContain('À créer');
  },
};

export const DockerBuild: Story = {
  decorators: [withData({ secrets: ['REGISTRY_USER'] })],
  play: async (context) => {
    const doc = context.canvasElement.ownerDocument;
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('.pipeline-editor__add-job button')!.click();
    const tile = await waitFor(() => {
      const el = doc.querySelector<HTMLButtonElement>('[data-tile="docker-build"] .tile-picker__choose');
      if (!el) throw new Error('catalogue not open yet');
      return el;
    });
    tile.click();
    await waitFor(() => expect(doc.querySelector('.tile-form__preview')?.textContent).toContain('docker -H "$DOCKER_HOST" build'));
  },
};

/**
 * With Kubernetes running the jobs, the tiles that read secrets or work on the repository's files are disabled, and say
 * why.
 */
export const CatalogueWithKubernetes: Story = {
  decorators: [withData({ engine: 'kubernetes' })],
  play: async (context) => {
    const doc = context.canvasElement.ownerDocument;
    await expectBoardLayout(context);
    context.canvasElement.querySelector<HTMLButtonElement>('.pipeline-editor__add-job button')!.click();
    await waitFor(() => expect(doc.querySelector<HTMLButtonElement>('[data-tile="ssh-run"] .tile-picker__choose')?.disabled).toBe(true));
    await expect(doc.querySelector<HTMLButtonElement>('[data-tile="rust-test"] .tile-picker__choose')?.disabled).toBe(true);
    await expect(doc.querySelector<HTMLButtonElement>('[data-tile="custom"] .tile-picker__choose')?.disabled).toBe(false);
  },
};

/** An empty pipeline with Kubernetes: no proposal and no template, since all of them work on the repository's files. */
export const EmptyWithKubernetes: Story = {
  decorators: [withData({ engine: 'kubernetes', file: of({ ...FILE, yaml: null }), profile: of(FERRISGIT_PROFILE) })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('fg-pipeline-starters [data-kubernetes]')).not.toBeNull());
    await expect(canvasElement.querySelector('[data-prediction]')).toBeNull();
    await expect(canvasElement.querySelector('.starters__card')).toBeNull();
  },
};
