import { Component, ElementRef, Injector, OnInit, TemplateRef, afterNextRender, computed, inject, input, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { FileUpload } from '@masmarino/gabarit/file-upload';
import { GbtDateTimePipe, GbtRelativeTimePipe, formatBytes as formatBytesFn } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { IconMarker } from '@masmarino/gabarit/icon-marker';
import { GbtInput } from '@masmarino/gabarit/input';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Switch } from '@masmarino/gabarit/switch';
import { Textarea } from '@masmarino/gabarit/textarea';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { ReleaseAsset, ReleaseDetail as ReleaseDetailResponse, ReleasesService, releaseStatus } from '../releases.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';
import { MarkdownView } from '../../shared/markdown-view/markdown-view';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';
import { activeLocale, t } from '../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  selector: 'fg-release-detail',
  standalone: true,
  imports: [TranslocoPipe, 
    FormsModule,
    RouterLink,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    MarkdownView,
    PageLayout,
    PageHeader,
    Panel,
    StatusBadge,
    UserChip,
    Badge,
    Button,
    Card,
    ConfirmDangerModal,
    DescriptionList,
    EmptyState,
    FileUpload,
    GbtInput,
    Icon,
    IconMarker,
    Skeleton,
    Switch,
    Textarea,
  ],
  templateUrl: './release-detail.html',
  styleUrl: './release-detail.scss',
})
export class ReleaseDetail implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();
  tagName = input.required<string>();

  private releases = inject(ReleasesService);
  private repositoryContext = inject(RepositoryContextService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  private router = inject(Router);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private injector = inject(Injector);

  protected release = signal<ReleaseDetailResponse | null>(null);
  protected notFound = signal(false);
  protected role = computed(() => this.repositoryContext.current()?.role ?? null);
  // Public so the spec can call it.
  canManage = computed(() => this.role() === 'owner' || this.role() === 'maintainer');

  protected status = computed(() => {
    const release = this.release();
    return release ? releaseStatus(release) : 'published';
  });
  protected headerDate = computed(() => {
    const release = this.release();
    return release ? (release.publishedAt ?? release.createdAt) : '';
  });
  protected releasesLink = computed(() => ['/repositories', ...this.path(), '-', 'releases']);
  protected treeLink = computed(() => ['/repositories', ...this.path(), '-', 'tree', this.tagName()]);
  protected shortSha = computed(() => this.release()?.targetCommitSha?.slice(0, 7) ?? null);
  protected totalSize = computed(() => this.formatBytes((this.release()?.assets ?? []).reduce((sum, asset) => sum + asset.sizeBytes, 0)));
  protected readonly formatBytes = (bytes: number) => formatBytesFn(bytes, activeLocale(), { binaryUnits: 'legacy' });

  private createdTemplate = viewChild.required<TemplateRef<unknown>>('createdTemplate');
  private publishedTemplate = viewChild.required<TemplateRef<unknown>>('publishedTemplate');
  private notPublishedTemplate = viewChild.required<TemplateRef<unknown>>('notPublishedTemplate');

  protected publicationFacts = computed<DescriptionListEntry[]>(() => {
    const release = this.release();
    if (!release) return [];
    return [
      { term: t('common.created'), value: this.createdTemplate() },
      { term: t('common.published'), value: release.publishedAt ? this.publishedTemplate() : this.notPublishedTemplate() },
    ];
  });
  protected downloadFacts = computed<DescriptionListEntry[]>(() => {
    const release = this.release();
    if (!release) return [];
    return [
      { term: t('common.files'), value: String(release.assets.length) },
      { term: t('common.totalSize'), value: this.totalSize() },
    ];
  });
  protected readonly skeletonLines = ['92%', '84%', '60%', '88%', '45%'];

  protected editing = signal(false);
  protected editTitle = signal('');
  protected editNotes = signal('');
  protected editPrerelease = signal(false);
  protected saving = signal(false);
  protected publishing = signal(false);

  protected uploading = signal<string | null>(null);
  protected selectedFiles = signal<File[]>([]);

  protected deleteReleaseOpen = signal(false);
  protected assetPendingDelete = signal<ReleaseAsset | null>(null);
  /** True while a confirmed file deletion runs: the confirmation stays open and ignores a second click. */
  protected deletingAsset = signal(false);

  ngOnInit(): void {
    this.load();
  }

  private load(): void {
    this.releases.detail(this.repositoryId(), this.tagName()).subscribe({
      next: (release) => {
        this.release.set(release);
        this.pageTitle.set(release.title);
      },
      error: () => this.notFound.set(true),
    });
  }

  /** Moves focus once the next render has put `selector` on the page (a control replaced the one that had focus). */
  private focusAfterRender(selector: string): void {
    afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>(selector)?.focus(), { injector: this.injector });
  }

  startEdit(): void {
    const release = this.release();
    if (!release) return;
    this.editTitle.set(release.title);
    this.editNotes.set(release.notes);
    this.editPrerelease.set(release.prerelease);
    this.editing.set(true);
    this.focusAfterRender('.release-detail__edit-title input');
  }

  cancelEdit(): void {
    this.editing.set(false);
    this.focusAfterRender('.release-detail__edit-button button');
  }

  save(): void {
    const title = this.editTitle().trim();
    if (!title) {
      this.toast.show(t('releases.titleRequired'), 'error');
      return;
    }
    this.saving.set(true);
    this.releases
      .update(this.repositoryId(), this.tagName(), { title, notes: this.editNotes(), prerelease: this.editPrerelease() })
      .subscribe({
        next: (release) => {
          this.saving.set(false);
          this.release.set(release);
          this.pageTitle.set(release.title);
          this.editing.set(false);
          this.toast.show(t('releases.updated'));
          this.focusAfterRender('.release-detail__edit-button button');
        },
        error: () => {
          this.saving.set(false);
          this.toast.show(t('releases.saveFailed'), 'error');
        },
      });
  }

  publish(): void {
    this.publishing.set(true);
    this.releases.update(this.repositoryId(), this.tagName(), { draft: false }).subscribe({
      next: (release) => {
        this.publishing.set(false);
        this.release.set(release);
        this.toast.show(t('releases.published'));
        // The button is gone with the draft, so keep the keyboard user in the header's actions.
        this.focusAfterRender('.release-detail__edit-button button');
      },
      error: () => {
        this.publishing.set(false);
        this.toast.show(t('releases.publishFailed'), 'error');
      },
    });
  }

  deleteRelease(): void {
    this.deleteReleaseOpen.set(true);
  }

  protected confirmDeleteRelease(): void {
    this.deleteReleaseOpen.set(false);
    this.releases.delete(this.repositoryId(), this.tagName()).subscribe({
      next: () => {
        this.toast.show(t('releases.deleted'));
        this.router.navigate(this.releasesLink());
      },
      error: () => this.toast.show(t('releases.deleteFailed'), 'error'),
    });
  }

  onFilesSelected(files: File[]): void {
    const file = files[0];
    if (!file) return;
    this.uploading.set(file.name);
    this.releases.uploadAsset(this.repositoryId(), this.tagName(), file).subscribe({
      next: (asset) => {
        this.uploading.set(null);
        this.selectedFiles.set([]);
        this.release.update((release) => (release ? { ...release, assets: [...release.assets, asset] } : release));
        this.toast.show(t('releases.fileAdded'));
      },
      error: () => {
        this.uploading.set(null);
        this.selectedFiles.set([]);
        this.toast.show(t('releases.uploadFailed'), 'error');
      },
    });
  }

  deleteAsset(asset: ReleaseAsset): void {
    this.assetPendingDelete.set(asset);
  }

  protected confirmDeleteAsset(): void {
    const asset = this.assetPendingDelete();
    if (!asset || this.deletingAsset()) return;
    this.deletingAsset.set(true);
    this.releases.deleteAsset(this.repositoryId(), this.tagName(), asset.id).subscribe({
      next: () => {
        this.deletingAsset.set(false);
        this.assetPendingDelete.set(null);
        this.release.update((release) => (release ? { ...release, assets: release.assets.filter((a) => a.id !== asset.id) } : release));
        this.toast.show(t('releases.fileDeleted'));
      },
      // The confirmation covers the page, so an error has to close it first to be visible.
      error: () => {
        this.deletingAsset.set(false);
        this.assetPendingDelete.set(null);
        this.toast.show(t('releases.fileDeleteFailed'), 'error');
      },
    });
  }

  /** Goes through HttpClient so the auth interceptor adds the JWT (a plain `<a [href]>` would 401), then saved through a synthetic `<a download>`. */
  download(asset: ReleaseAsset): void {
    this.releases.downloadAsset(this.repositoryId(), this.tagName(), asset.id).subscribe({
      next: (blob) => {
        const objectUrl = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = objectUrl;
        link.download = asset.filename;
        link.click();
        URL.revokeObjectURL(objectUrl);
      },
      error: () => this.toast.show(t('releases.downloadFailed'), 'error'),
    });
  }
}
