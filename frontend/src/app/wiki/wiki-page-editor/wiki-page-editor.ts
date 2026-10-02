import { Component, OnInit, computed, inject, input, signal } from '@angular/core';
import { Router } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { Alert, Button, GbtInput, Icon, ListCard, PageHeader, GbtToastService } from '@masmarino/gabarit';
import { SaveWikiPageOptions, WikiList, WikiService, WikiPageSummary } from '../wiki.service';
import { WikiLayout } from '../wiki-layout/wiki-layout';
import { titleFromSlug, wikiLink } from '../wiki-links';
import { PageTitleService } from '../../shell/page-title.service';
import { MarkdownView } from '../../shared/markdown-view/markdown-view';

const SLUG_PATTERN = /^[A-Za-z0-9_][A-Za-z0-9_-]{0,99}$/;

const CONFLICT_MESSAGE = "Quelqu'un d'autre a modifié cette page depuis que vous l'avez ouverte. Rechargez et réappliquez vos changements.";

let nextId = 0;

@Component({
  selector: 'fg-wiki-page-editor',
  standalone: true,
  imports: [FormsModule, Alert, Button, GbtInput, Icon, ListCard, MarkdownView, PageHeader, WikiLayout],
  templateUrl: './wiki-page-editor.html',
  styleUrl: './wiki-page-editor.scss',
})
export class WikiPageEditor implements OnInit {
  repositoryId = input.required<string>();
  slug = input.required<string | null>();
  path = input.required<string[]>();

  private wiki = inject(WikiService);
  private router = inject(Router);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);

  protected readonly ids = { source: `wiki-editor-source-${nextId}`, preview: `wiki-editor-preview-${nextId++}` };

  protected isCreate = computed(() => this.slug() === null);
  protected saving = signal(false);
  /** Set by a 409: the explanation stays above the editor until the next save attempt. */
  protected conflict = signal(false);
  // Public so the spec can call it.
  newSlug = signal('');
  content = signal('');
  message = signal('');
  baseSha = signal<string | null>(null);
  // Only populated in create mode, to block creating a page under an existing slug.
  existingPages = signal<WikiPageSummary[]>([]);
  private loadedTitle = signal<string | null>(null);

  protected heading = computed(() => (this.isCreate() ? 'Nouvelle page' : `Modifier ${this.loadedTitle() ?? titleFromSlug(this.slug()!)}`));
  protected messagePlaceholder = computed(() => `Update ${this.isCreate() ? this.newSlug().trim() || 'Nom-de-la-page' : this.slug()}`);

  ngOnInit(): void {
    this.pageTitle.set(this.isCreate() ? 'Nouvelle page' : `Modifier ${this.slug()}`);
    if (!this.isCreate()) {
      this.wiki.detail(this.repositoryId(), this.slug()!).subscribe({
        next: (detail) => {
          this.content.set(detail.content);
          this.baseSha.set(detail.headSha);
          this.loadedTitle.set(detail.title);
        },
        error: () => this.toast.show('Impossible de charger la page.', 'error'),
      });
    }
  }

  protected onListLoaded(list: WikiList): void {
    if (this.isCreate()) {
      this.baseSha.set(list.headSha);
      this.existingPages.set(list.pages);
    }
  }

  protected onListFailed(): void {
    if (this.isCreate()) {
      this.toast.show('Impossible de charger la page.', 'error');
    }
  }

  protected isSlugValid(): boolean {
    return SLUG_PATTERN.test(this.newSlug());
  }

  save(): void {
    const slug = this.isCreate() ? this.newSlug().trim() : this.slug()!;
    if (this.isCreate() && !this.isSlugValid()) {
      this.toast.show('Le nom de la page doit contenir uniquement des lettres, chiffres, "-" et "_"', 'error');
      return;
    }
    if (this.isCreate() && this.existingPages().some((page) => page.slug === slug)) {
      // The create-or-update PUT would silently overwrite an existing page, so block it here.
      this.toast.show(`Une page nommée « ${slug} » existe déjà — modifiez-la plutôt que d'en créer une nouvelle.`, 'error');
      return;
    }
    this.saving.set(true);
    this.conflict.set(false);
    const options: SaveWikiPageOptions = { content: this.content(), baseSha: this.baseSha() };
    const message = this.message().trim();
    if (message) {
      options.message = message;
    }
    this.wiki.save(this.repositoryId(), slug, options).subscribe({
      next: () => {
        this.toast.show(this.isCreate() ? 'Page wiki créée.' : 'Page wiki mise à jour.');
        this.router.navigate(wikiLink(this.path(), slug));
      },
      error: (err: { status?: number }) => {
        this.saving.set(false);
        if (err.status === 409) {
          // Keeps `content()` so the user's text isn't lost. They can reload the current version and reapply their changes.
          this.conflict.set(true);
          this.toast.show(CONFLICT_MESSAGE, 'error');
        } else {
          this.toast.show('Impossible d’enregistrer la page', 'error');
        }
      },
    });
  }

  protected cancel(): void {
    const back = this.isCreate() ? [] : [this.slug()!];
    void this.router.navigate(wikiLink(this.path(), ...back));
  }

  protected readonly conflictMessage = CONFLICT_MESSAGE;
}
