import { Component, Type } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { of } from 'rxjs';
import { RepositorySettings } from './repository-settings';
import { RepositorySettingsService } from '../repository-settings.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit';
import { LabelsService } from '../../labels/labels.service';
import { MilestonesService } from '../../milestones/milestones.service';
import { RepositoryPipelineSettings } from '../repository-pipeline-settings/repository-pipeline-settings';
import { RepositoryCiVariables } from '../repository-ci-variables/repository-ci-variables';
import { RepositoryWebhooks } from '../repository-webhooks/repository-webhooks';
import { RepositoryCollaboratorsSettings } from '../repository-collaborators-settings/repository-collaborators-settings';
import { RepositoryLabelsSettings } from '../repository-labels-settings/repository-labels-settings';
import { RepositoryMilestonesSettings } from '../repository-milestones-settings/repository-milestones-settings';

@Component({
  imports: [RepositorySettings],
  template: `<fg-repository-settings repositoryId="repo-1" />`,
})
class SettingsHost {}

const SETTINGS_URL = '/repositories/florian/ferrisgit/-/settings';

const PANELS: [string, Type<unknown>][] = [
  ['pipeline', RepositoryPipelineSettings],
  ['variables', RepositoryCiVariables],
  ['webhooks', RepositoryWebhooks],
  ['collaborators', RepositoryCollaboratorsSettings],
  ['labels', RepositoryLabelsSettings],
  ['milestones', RepositoryMilestonesSettings],
];

describe('RepositorySettings', () => {
  async function setup(url = SETTINGS_URL) {
    // Every HTTP-backed service of the 6 sections needs a stub, since each is rendered when selected.
    const repositorySettingsStub = {
      get: vi.fn(() => of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 0 })),
      update: vi.fn(() => of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 0 })),
      listCiVariables: vi.fn(() => of([])),
      setCiVariable: vi.fn(() => of({ id: 'v1', key: 'KEY', masked: true })),
      deleteCiVariable: vi.fn(() => of(undefined)),
      listCollaborators: vi.fn(() => of([])),
      addCollaborator: vi.fn(() => of(undefined)),
      setCollaboratorRole: vi.fn(() => of(undefined)),
      removeCollaborator: vi.fn(() => of(undefined)),
      listWebhooks: vi.fn(() => of([])),
      createWebhook: vi.fn(() => of(undefined)),
      deleteWebhook: vi.fn(() => of(undefined)),
      listWebhookDeliveries: vi.fn(() => of([])),
    };
    const pageTitleStub = { set: vi.fn() };
    const labelsServiceStub = {
      listForRepository: vi.fn(() => of([])),
      create: vi.fn(() => of({ id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' })),
      delete: vi.fn(() => of(undefined)),
    };
    const milestonesServiceStub = {
      listForRepository: vi.fn(() => of([])),
      create: vi.fn(() =>
        of({
          id: 'm1',
          title: 'v1.0',
          description: '',
          dueDate: null,
          state: 'open',
          repositoryId: 'repo-1',
          groupId: null,
          createdAt: '2026-01-01T00:00:00Z',
        }),
      ),
      delete: vi.fn(() => of(undefined)),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        provideRouter([{ path: 'repositories/**', component: SettingsHost }]),
        { provide: RepositorySettingsService, useValue: repositorySettingsStub },
        { provide: PageTitleService, useValue: pageTitleStub },
        { provide: LabelsService, useValue: labelsServiceStub },
        { provide: MilestonesService, useValue: milestonesServiceStub },
        { provide: GbtToastService, useValue: toastStub },
      ],
    });
    const harness = await RouterTestingHarness.create(url);
    const el = () => harness.routeNativeElement as HTMLElement;
    const rendered = (type: Type<unknown>) => harness.routeDebugElement!.queryAll(By.directive(type));
    const navigate = async (target: string) => {
      await harness.navigateByUrl(target);
      harness.fixture.detectChanges();
      await harness.fixture.whenStable();
    };
    const current = () => Array.from(el().querySelectorAll('gbt-nav-tabs a[aria-current="page"]')).map((a) => a.querySelector('.gbt-nav-tab__label')?.textContent);
    return { harness, el, rendered, navigate, current, pageTitleStub, repositorySettingsStub };
  }

  it('sets the page title', async () => {
    const { pageTitleStub } = await setup();
    expect(pageTitleStub.set).toHaveBeenCalledWith('Réglages');
  });

  it('is a wide page: the "Réglages" h1 above a layout whose nav column holds the section navigation', async () => {
    const { el } = await setup();

    expect(el().querySelector('gbt-page-header h1')?.textContent?.trim()).toBe('Réglages');
    const layout = el().querySelector('gbt-page-layout')!;
    expect(layout.getAttribute('data-width')).toBe('wide');
    const nav = layout.querySelector('nav.gbt-page-layout__nav')!;
    expect(nav.getAttribute('aria-label')).toBe('Réglages du dépôt');
    expect(nav.querySelector('gbt-nav-tabs')).not.toBeNull();
    expect(el().querySelectorAll('nav')).toHaveLength(1);
  });

  it('lists the six sections, in order, as links to ?section=<key> (the default one on the bare path)', async () => {
    const { el } = await setup();

    const links = Array.from(el().querySelectorAll<HTMLAnchorElement>('gbt-nav-tabs a'));
    expect(links.map((a) => a.querySelector('.gbt-nav-tab__label')?.textContent)).toEqual([
      'Pipeline',
      'Variables CI/CD',
      'Webhooks',
      'Collaborateurs',
      'Labels',
      'Milestones',
    ]);
    expect(links.map((a) => a.getAttribute('href'))).toEqual([
      SETTINGS_URL,
      `${SETTINGS_URL}?section=variables`,
      `${SETTINGS_URL}?section=webhooks`,
      `${SETTINGS_URL}?section=collaborators`,
      `${SETTINGS_URL}?section=labels`,
      `${SETTINGS_URL}?section=milestones`,
    ]);
    expect(links.every((a) => a.querySelector('gbt-icon') !== null)).toBe(true);
  });

  it('shows the pipeline section by default, and only it', async () => {
    const { rendered, current } = await setup();

    expect(rendered(RepositoryPipelineSettings)).toHaveLength(1);
    expect(rendered(RepositoryPipelineSettings)[0].componentInstance.repositoryId()).toBe('repo-1');
    for (const [, type] of PANELS.slice(1)) {
      expect(rendered(type), `${type.name} should not be rendered`).toHaveLength(0);
    }
    expect(current()).toEqual(['Pipeline']);
  });

  it.each(PANELS)('?section=%s renders that section alone, with the repository id, and marks it in the nav', async (key, type) => {
    const { rendered, current } = await setup(`${SETTINGS_URL}?section=${key}`);

    expect(rendered(type)).toHaveLength(1);
    expect(rendered(type)[0].componentInstance.repositoryId()).toBe('repo-1');
    for (const [otherKey, other] of PANELS) {
      if (otherKey !== key) {
        expect(rendered(other), `${other.name} should not be rendered`).toHaveLength(0);
      }
    }
    expect(current()).toHaveLength(1);
  });

  it('falls back to the pipeline section for an unknown section', async () => {
    const { rendered, current } = await setup(`${SETTINGS_URL}?section=inconnue`);

    expect(rendered(RepositoryPipelineSettings)).toHaveLength(1);
    expect(rendered(RepositoryWebhooks)).toHaveLength(0);
    expect(current()).toEqual(['Pipeline']);
  });

  it('follows the query param: navigating swaps the section without recreating the page', async () => {
    const { harness, rendered, navigate, current, el } = await setup();
    const header = el().querySelector('gbt-page-header');

    await navigate(`${SETTINGS_URL}?section=labels`);
    expect(rendered(RepositoryLabelsSettings)).toHaveLength(1);
    expect(rendered(RepositoryPipelineSettings)).toHaveLength(0);
    expect(current()).toEqual(['Labels']);

    await navigate(SETTINGS_URL);
    expect(rendered(RepositoryPipelineSettings)).toHaveLength(1);
    expect(rendered(RepositoryLabelsSettings)).toHaveLength(0);
    expect(current()).toEqual(['Pipeline']);
    expect(harness.routeNativeElement?.querySelector('gbt-page-header')).toBe(header);
  });

  it('marks the current section by its query param alone: unrelated params and the fragment change nothing', async () => {
    const { current } = await setup(`${SETTINGS_URL}?page=2&section=variables#ajouter`);

    expect(current()).toEqual(['Variables CI/CD']);
  });

  it('lays the section links out as the vertical list of a nav column (Gabarit turns it into a row of tabs when the layout stacks)', async () => {
    const { el } = await setup();

    const links = Array.from(el().querySelectorAll<HTMLAnchorElement>('gbt-nav-tabs a'));
    expect(links).toHaveLength(6);
    expect(links.every((a) => a.getAttribute('data-orientation') === 'vertical')).toBe(true);
  });

  it('moves to a section when its nav link is clicked', async () => {
    const { harness, rendered, el } = await setup();

    el().querySelectorAll<HTMLAnchorElement>('gbt-nav-tabs a')[2].click();
    await harness.fixture.whenStable();
    harness.fixture.detectChanges();

    expect(rendered(RepositoryWebhooks)).toHaveLength(1);
    expect(rendered(RepositoryPipelineSettings)).toHaveLength(0);
  });

  it('loads a section only when it is shown', async () => {
    const { repositorySettingsStub, navigate } = await setup();
    expect(repositorySettingsStub.get).toHaveBeenCalledWith('repo-1');
    expect(repositorySettingsStub.listWebhooks).not.toHaveBeenCalled();

    await navigate(`${SETTINGS_URL}?section=webhooks`);
    expect(repositorySettingsStub.listWebhooks).toHaveBeenCalledWith('repo-1');
  });
});
