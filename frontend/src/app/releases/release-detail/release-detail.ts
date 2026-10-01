import { Component, ElementRef, Injector, OnInit, TemplateRef, afterNextRender, computed, inject, input, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import {
  Badge,
  Button,
  Card,
  ConfirmDangerModal,
  DescriptionList,
  DescriptionListEntry,
  EmptyState,
  FileUpload,
  GbtDateTimePipe,
  GbtInput,
  GbtRelativeTimePipe,
  GbtToastService,
  Icon,
  IconMarker,
  PageHeader,
  PageLayout,
  Panel,
  Skeleton,
  Switch,
  Textarea,
  UserChip,
  formatBytes as formatBytesFn,
} from '@masmarino/gabarit';
import { ReleaseAsset, ReleaseDetail as ReleaseDetailResponse, ReleasesService, releaseStatus } from '../releases.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';
import { MarkdownView } from '../../shared/markdown-view/markdown-view';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';

@Component({
  selector: 'fg-release-detail',
  standalone: true,
  imports: [
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
  // Not `protected` because the spec calls it directly.
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
  protected readonly formatBytes = (bytes: number) => formatBytesFn(bytes, 'fr', { binaryUnits: 'legacy' });

  private createdTemplate = viewChild.required<TemplateRef<unknown>>('createdTemplate');
  private publishedTemplate = viewChild.required<TemplateRef<unknown>>('publishedTemplate');
  private notPublishedTemplate = viewChild.required<TemplateRef<unknown>>('notPublishedTemplate');

  protected publicationFacts = computed<DescriptionListEntry[]>(() => {
    const release = this.release();
    if (!release) return [];
    return [
      { term: 'Créée', value: this.createdTemplate() },
      { term: 'Publiée', value: release.publishedAt ? this.publishedTemplate() : this.notPublishedTemplate() },
    ];
  });
  protected downloadFacts = computed<DescriptionListEntry[]>(() => {
    const release = this.release();
    if (!release) return [];
    return [
      { term: 'Fichiers', value: String(release.assets.length) },
      { term: 'Taille totale', value: this.totalSize() },
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
  /** True while a confirmed file deletion runs: its confirmation stays open and ignores a second click. */
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
      this.toast.show('Le titre est requis', 'error');
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
          this.toast.show('Release mise à jour.');
          this.focusAfterRender('.release-detail__edit-button button');
        },
        error: () => {
          this.saving.set(false);
          this.toast.show("Impossible d'enregistrer les modifications", 'error');
        },
      });
  }

  publish(): void {
    this.publishing.set(true);
    this.releases.update(this.repositoryId(), this.tagName(), { draft: false }).subscribe({
      next: (release) => {
        this.publishing.set(false);
        this.release.set(release);
        this.toast.show('Release publiée.');
        // The button is gone with the draft: keep the keyboard user in the header's actions.
        this.focusAfterRender('.release-detail__edit-button button');
      },
      error: () => {
        this.publishing.set(false);
        this.toast.show('Impossible de publier la release.', 'error');
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
        this.toast.show('Release supprimée.');
        this.router.navigate(this.releasesLink());
      },
      error: () => this.toast.show('Impossible de supprimer la release.', 'error'),
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
        this.toast.show('Fichier ajouté.');
      },
      error: () => {
        this.uploading.set(null);
        this.selectedFiles.set([]);
        this.toast.show("Échec de l'envoi du fichier", 'error');
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
        this.toast.show('Fichier supprimé.');
      },
      // The confirmation covers the page, so an error must close it first to be visible.
      error: () => {
        this.deletingAsset.set(false);
        this.assetPendingDelete.set(null);
        this.toast.show('Impossible de supprimer le fichier.', 'error');
      },
    });
  }

  /** Fetched via `HttpClient` so the auth interceptor attaches the JWT (a plain `<a [href]>` would 401), then saved through a synthetic `<a download>`. */
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
      error: () => this.toast.show('Échec du téléchargement', 'error'),
    });
  }
}
