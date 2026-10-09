import { Component, computed, inject, input, OnInit } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { map } from 'rxjs';
import { PageTitleService } from '../../shell/page-title.service';
import { RepositoryGeneralSettings } from '../repository-general-settings/repository-general-settings';
import { RepositoryPipelineSettings } from '../repository-pipeline-settings/repository-pipeline-settings';
import { RepositoryCiVariables } from '../repository-ci-variables/repository-ci-variables';
import { RepositoryWebhooks } from '../repository-webhooks/repository-webhooks';
import { RepositoryCollaboratorsSettings } from '../repository-collaborators-settings/repository-collaborators-settings';
import { RepositoryLabelsSettings } from '../repository-labels-settings/repository-labels-settings';
import { RepositoryMilestonesSettings } from '../repository-milestones-settings/repository-milestones-settings';
import { NavTab, NavTabs } from '@masmarino/gabarit/nav-tabs';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

export type SettingsSectionKey = 'general' | 'pipeline' | 'variables' | 'webhooks' | 'collaborators' | 'labels' | 'milestones';

const SECTIONS: { key: SettingsSectionKey; label: string; icon: string }[] = [
  {
    key: 'general',
    get label() {
      return t('repositories.sections.general');
    },
    icon: 'info',
  },
  {
    key: 'pipeline',
    get label() {
      return t('repositories.sections.pipeline');
    },
    icon: 'play',
  },
  {
    key: 'variables',
    get label() {
      return t('repositories.sections.variables');
    },
    icon: 'key',
  },
  {
    key: 'webhooks',
    get label() {
      return t('repositories.sections.webhooks');
    },
    icon: 'globe',
  },
  {
    key: 'collaborators',
    get label() {
      return t('repositories.sections.collaborators');
    },
    icon: 'user',
  },
  {
    key: 'labels',
    get label() {
      return t('repositories.sections.labels');
    },
    icon: 'tag',
  },
  {
    key: 'milestones',
    get label() {
      return t('repositories.sections.milestones');
    },
    icon: 'flag',
  },
];

const DEFAULT_SECTION: SettingsSectionKey = 'general';

@Component({
  selector: 'fg-repository-settings',
  standalone: true,
  imports: [TranslocoPipe, 
    PageHeader,
    PageLayout,
    NavTab,
    NavTabs,
    RouterLink,
    RepositoryGeneralSettings,
    RepositoryPipelineSettings,
    RepositoryCiVariables,
    RepositoryWebhooks,
    RepositoryCollaboratorsSettings,
    RepositoryLabelsSettings,
    RepositoryMilestonesSettings,
  ],
  templateUrl: './repository-settings.html',
  styleUrl: './repository-settings.scss',
})
export class RepositorySettings implements OnInit {
  repositoryId = input.required<string>();

  private pageTitle = inject(PageTitleService);
  private route = inject(ActivatedRoute);

  protected readonly sections = SECTIONS.map((section) => ({
    ...section,
    queryParams: section.key === DEFAULT_SECTION ? undefined : { section: section.key },
  }));

  private requestedSection = toSignal(this.route.queryParamMap.pipe(map((params) => params.get('section'))), { initialValue: null });

  protected activeSection = computed<SettingsSectionKey>(() => {
    const requested = this.requestedSection();
    return SECTIONS.find((section) => section.key === requested)?.key ?? DEFAULT_SECTION;
  });

  ngOnInit(): void {
    this.pageTitle.set(t('nav.settings'));
  }
}
