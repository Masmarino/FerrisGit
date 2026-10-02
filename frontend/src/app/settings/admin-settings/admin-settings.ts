import { Component, computed, inject, OnInit } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { map } from 'rxjs';
import { Alert, Button, Card, CardHeader, NavTab, NavTabs, PageHeader, PageLayout, Skeleton } from '@masmarino/gabarit';
import { PageTitleService } from '../../shell/page-title.service';
import { ExecutionSettings } from '../execution-settings/execution-settings';
import { SecuritySettings } from '../security-settings/security-settings';
import { SettingsEditor } from '../settings-editor';
import { SmtpSettings } from '../smtp-settings/smtp-settings';

type SectionKey = 'execution' | 'security' | 'email';

const SECTIONS: { key: SectionKey; label: string; icon: string }[] = [
  { key: 'execution', label: 'Exécution', icon: 'server' },
  { key: 'security', label: 'Sécurité', icon: 'lock' },
  { key: 'email', label: 'E-mail', icon: 'mail' },
];

const DEFAULT_SECTION: SectionKey = 'execution';

/** The instance settings: a tab per section, each its own component, all editing through one `SettingsEditor`. */
@Component({
  selector: 'fg-admin-settings',
  standalone: true,
  imports: [PageHeader, PageLayout, NavTab, NavTabs, RouterLink, Card, CardHeader, Alert, Button, Skeleton, ExecutionSettings, SecuritySettings, SmtpSettings],
  providers: [SettingsEditor],
  templateUrl: './admin-settings.html',
  styleUrl: './admin-settings.scss',
})
export class AdminSettings implements OnInit {
  protected editor = inject(SettingsEditor);
  private pageTitle = inject(PageTitleService);
  private route = inject(ActivatedRoute);

  protected readonly sections = SECTIONS.map((section) => ({
    ...section,
    queryParams: section.key === DEFAULT_SECTION ? undefined : { section: section.key },
  }));
  protected readonly skeletonCards = [
    { title: '9rem', help: '65%' },
    { title: '7rem', help: '50%' },
  ];

  private requestedSection = toSignal(this.route.queryParamMap.pipe(map((params) => params.get('section'))), { initialValue: null });

  protected activeSection = computed<SectionKey>(() => {
    const requested = this.requestedSection();
    return SECTIONS.find((section) => section.key === requested)?.key ?? DEFAULT_SECTION;
  });

  ngOnInit(): void {
    this.pageTitle.set("Réglages de l'instance");
    this.editor.load();
  }
}
