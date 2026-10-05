import { Component, OnInit, computed, inject, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ReleasesService, TagSummary } from '../releases.service';
import { BranchInfo, MergeRequestsService } from '../../merge-requests/merge-requests.service';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { Modal } from '@masmarino/gabarit/modal';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { Switch } from '@masmarino/gabarit/switch';
import { Textarea } from '@masmarino/gabarit/textarea';
import { GbtToastService } from '@masmarino/gabarit/toaster';

const NEW_TAG_SENTINEL = '__new__';

@Component({
  selector: 'fg-create-release-modal',
  standalone: true,
  imports: [FormsModule, Modal, GbtInput, Button, Switch, Select, Textarea, Alert, Icon],
  templateUrl: './create-release-modal.html',
  styleUrl: './create-release-modal.scss',
})
export class CreateReleaseModal implements OnInit {
  repositoryId = input.required<string>();

  private releases = inject(ReleasesService);
  private mergeRequests = inject(MergeRequestsService);
  private toast = inject(GbtToastService);
  close = output<void>();
  created = output<void>();

  protected existingTags = signal<TagSummary[]>([]);
  protected branches = signal<BranchInfo[]>([]);
  // Public so the spec can call it.
  tagChoice = signal<string>(NEW_TAG_SENTINEL);
  isNewTag = computed(() => this.tagChoice() === NEW_TAG_SENTINEL);
  newTagName = signal('');
  protected targetBranch = signal('');
  title = signal('');
  protected notes = signal('');
  protected draft = signal(true);
  protected prerelease = signal(false);
  /** A create (or delete-and-retry) request is in flight: the submit button shows it and a second submission is ignored. */
  protected creating = signal(false);
  error = signal('');
  // Set only when a brand-new tag name fails with a 400 ("this tag exists already, at another commit"). An existing tag
  // picked from the dropdown resubmits its own sha, so it can't conflict.
  conflictedTagName = signal<string | null>(null);

  protected tagOptions = computed<SelectOption<string>[]>(() => [
    { value: NEW_TAG_SENTINEL, label: 'Nouveau tag' },
    ...this.existingTags().map((t) => ({ value: t.name, label: t.name })),
  ]);
  protected branchOptions = computed<SelectOption<string>[]>(() => this.branches().map((b) => ({ value: b.name, label: b.name })));

  protected targetDescription = computed(() => {
    const sha = this.resolvedTargetSha();
    if (!sha) {
      return null;
    }
    const short = sha.slice(0, 7);
    return this.isNewTag() ? `Le tag pointera sur le dernier commit de ${this.targetBranch()} (${short})` : `Ce tag pointe sur le commit ${short}`;
  });

  ngOnInit(): void {
    this.releases.listTags(this.repositoryId()).subscribe({ next: (tags) => this.existingTags.set(tags), error: () => {} });
    this.mergeRequests.listBranches(this.repositoryId()).subscribe({
      next: (branches) => {
        this.branches.set(branches);
        const defaultBranch = branches.find((b) => b.isDefault);
        if (defaultBranch) {
          this.targetBranch.set(defaultBranch.name);
        }
      },
      error: () => {},
    });
  }

  resolvedTargetSha(): string | null {
    if (this.isNewTag()) {
      return this.branches().find((b) => b.name === this.targetBranch())?.tipSha ?? null;
    }
    return this.existingTags().find((t) => t.name === this.tagChoice())?.targetSha ?? null;
  }

  submit(): void {
    if (this.creating()) {
      return;
    }
    const isNewTag = this.isNewTag();
    const tagName = isNewTag ? this.newTagName().trim() : this.tagChoice();
    const targetCommitSha = this.resolvedTargetSha();
    const title = this.title().trim();
    if (!tagName || !targetCommitSha || !title) {
      this.error.set('Un nom de tag, une cible, et un titre sont requis');
      return;
    }
    this.conflictedTagName.set(null);
    this.creating.set(true);
    this.releases
      .create(this.repositoryId(), { tagName, targetCommitSha, title, notes: this.notes(), draft: this.draft(), prerelease: this.prerelease() })
      .subscribe({
        next: () => {
          this.creating.set(false);
          this.toast.show('Release créée.');
          this.created.emit();
        },
        error: (err: { status?: number }) => {
          this.creating.set(false);
          if (isNewTag && err.status === 400) {
            this.conflictedTagName.set(tagName);
            this.error.set(`Le tag « ${tagName} » existe déjà sur un autre commit.`);
          } else {
            this.error.set("Impossible de créer la release — ce tag existe peut-être déjà sur un autre commit");
          }
        },
      });
  }

  deleteConflictedTagAndRetry(): void {
    const tagName = this.conflictedTagName();
    if (!tagName || this.creating()) return;
    this.creating.set(true);
    this.releases.deleteTag(this.repositoryId(), tagName).subscribe({
      next: () => {
        this.creating.set(false);
        this.conflictedTagName.set(null);
        this.error.set('');
        this.submit();
      },
      error: () => {
        this.creating.set(false);
        this.error.set(`Impossible de supprimer le tag « ${tagName} » — il est peut-être encore utilisé par une release`);
      },
    });
  }
}
