import { Component, computed, HostListener, input, output, signal } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Comment, FileDiff, SplitDiffRow } from '../merge-requests.service';
import { Badge, Button, Textarea } from '@masmarino/gabarit';

interface CommentThread {
  root: Comment;
  replies: Comment[];
}

type DiffLayout = 'split' | 'old' | 'new';

interface HunkView {
  rows: SplitDiffRow[];
  header: string | null;
}

function hunkHeader(rows: SplitDiffRow[]): string | null {
  const oldLines = rows.map((row) => row.oldLine).filter((line): line is number => line !== null);
  const newLines = rows.map((row) => row.newLine).filter((line): line is number => line !== null);
  if ((oldLines[0] ?? 1) <= 1 && (newLines[0] ?? 1) <= 1) {
    return null;
  }
  const ranges = [oldLines.length ? `-${oldLines[0]},${oldLines.length}` : null, newLines.length ? `+${newLines[0]},${newLines.length}` : null];
  return `@@ ${ranges.filter((range) => range !== null).join(' ')} @@`;
}

@Component({
  selector: 'fg-file-diff-view',
  standalone: true,
  imports: [NgTemplateOutlet, FormsModule, Badge, Button, Textarea],
  templateUrl: './file-diff-view.html',
  styleUrl: './file-diff-view.scss',
})
export class FileDiffView {
  file = input.required<FileDiff>();
  comments = input<Comment[]>([]);
  canWrite = input<boolean>(false);

  commentAdded = output<{ filePath: string; lineNumber: number; endLine?: number; side: 'old' | 'new'; body: string; suggestedContent?: string }>();

  replyAdded = output<{ replyToId: string; body: string }>();

  resolveToggled = output<{ commentId: string; resolved: boolean }>();

  applySuggestionClicked = output<{ commentId: string }>();

  // An added file has nothing on the old side and a deleted one nothing on the new side: show the one side as a single wide
  // column. Checked against the rows too, so a file whose rows contradict its change type still gets both sides.
  protected layout = computed<DiffLayout>(() => {
    const file = this.file();
    const rows = file.hunks.flatMap((hunk) => hunk.rows);
    if (file.change === 'added' && rows.every((row) => row.oldContent === null)) {
      return 'new';
    }
    if (file.change === 'deleted' && rows.every((row) => row.newContent === null)) {
      return 'old';
    }
    return 'split';
  });
  protected showOld = computed(() => this.layout() !== 'new');
  protected showNew = computed(() => this.layout() !== 'old');
  protected columnCount = computed(() => (this.layout() === 'split' ? 4 : 2));

  protected hunks = computed<HunkView[]>(() => this.file().hunks.map((hunk) => ({ rows: hunk.rows, header: hunkHeader(hunk.rows) })));

  // Each key holds an array because independent root comments (different reviewers, or one twice) can anchor to the same file/line/side.
  protected threadsByKey = computed<Map<string, CommentThread[]>>(() => {
    const roots = this.comments().filter((c) => c.replyToId === null);
    const repliesByRoot = new Map<string, Comment[]>();
    for (const c of this.comments()) {
      if (c.replyToId !== null) {
        repliesByRoot.set(c.replyToId, [...(repliesByRoot.get(c.replyToId) ?? []), c]);
      }
    }
    const map = new Map<string, CommentThread[]>();
    for (const root of roots) {
      if (root.lineNumber === null || root.side === null) {
        continue; // General comments are rendered in MergeRequestDetail, not per line here.
      }
      const key = `${root.endLine ?? root.lineNumber}:${root.side}`;
      const thread = { root, replies: repliesByRoot.get(root.id) ?? [] };
      map.set(key, [...(map.get(key) ?? []), thread]);
    }
    return map;
  });

  protected threadsAt(lineNumber: number, side: 'old' | 'new'): CommentThread[] {
    return this.threadsByKey().get(`${lineNumber}:${side}`) ?? [];
  }

  protected toggleResolve(thread: CommentThread): void {
    this.resolveToggled.emit({ commentId: thread.root.id, resolved: !thread.root.resolved });
  }

  // A resolved thread starts collapsed. That state is local to the UI and not saved.
  protected expandedResolved = signal<ReadonlySet<string>>(new Set());

  protected isExpanded(rootId: string): boolean {
    return this.expandedResolved().has(rootId);
  }

  protected toggleExpanded(rootId: string): void {
    const next = new Set(this.expandedResolved());
    if (next.has(rootId)) {
      next.delete(rootId);
    } else {
      next.add(rootId);
    }
    this.expandedResolved.set(next);
  }

  // Keyed by thread root id: one shared signal would echo any typed draft into every open reply box.
  protected replyDrafts = signal<Record<string, string>>({});

  protected replyBodyFor(rootId: string): string {
    return this.replyDrafts()[rootId] ?? '';
  }

  protected setReplyBody(rootId: string, value: string): void {
    this.replyDrafts.update((current) => ({ ...current, [rootId]: value }));
  }

  protected submitReply(rootId: string): void {
    const body = this.replyBodyFor(rootId).trim();
    if (!body) {
      return;
    }
    this.replyAdded.emit({ replyToId: rootId, body });
    this.setReplyBody(rootId, '');
  }

  protected composingAt = signal<{ startLine: number; endLine: number; side: 'old' | 'new' } | null>(null);
  protected newCommentBody = signal('');
  protected showSuggestionInput = signal(false);
  protected suggestionBody = signal('');
  protected selecting = signal<{ side: 'old' | 'new'; startLine: number; endLine: number } | null>(null);

  protected beginSelecting(lineNumber: number, side: 'old' | 'new'): void {
    this.selecting.set({ side, startLine: lineNumber, endLine: lineNumber });
  }

  protected extendSelecting(lineNumber: number, side: 'old' | 'new'): void {
    const current = this.selecting();
    if (current && current.side === side) {
      this.selecting.set({ ...current, endLine: lineNumber });
    }
  }

  @HostListener('document:mouseup')
  finishSelecting(): void {
    const range = this.selecting();
    if (!range) {
      return;
    }
    const startLine = Math.min(range.startLine, range.endLine);
    const endLine = Math.max(range.startLine, range.endLine);
    this.composingAt.set({ startLine, endLine, side: range.side });
    this.newCommentBody.set('');
    this.showSuggestionInput.set(false);
    this.suggestionBody.set('');
    this.selecting.set(null);
  }

  // The add-comment button only listens to mousedown/mouseover (drag to select a range), so Enter and Space did
  // nothing. `event.detail === 0` means a keyboard click. A real mouse click already opened the compose row through
  // beginSelecting/finishSelecting and must do nothing here, or a multi-line selection would collapse to one line.
  protected onAddCommentClick(event: MouseEvent, lineNumber: number, side: 'old' | 'new'): void {
    if (event.detail !== 0) {
      return;
    }
    this.beginSelecting(lineNumber, side);
    this.finishSelecting();
  }

  protected isSelecting(lineNumber: number, side: 'old' | 'new'): boolean {
    const range = this.selecting();
    if (!range || range.side !== side) {
      return false;
    }
    const startLine = Math.min(range.startLine, range.endLine);
    const endLine = Math.max(range.startLine, range.endLine);
    return lineNumber >= startLine && lineNumber <= endLine;
  }

  protected cancelComposing(): void {
    this.composingAt.set(null);
    this.showSuggestionInput.set(false);
    this.suggestionBody.set('');
  }

  protected toggleSuggestionInput(): void {
    const target = this.composingAt();
    if (!target) {
      return;
    }
    const next = !this.showSuggestionInput();
    this.showSuggestionInput.set(next);
    if (next && !this.suggestionBody().trim()) {
      this.suggestionBody.set(this.originalRangeContent(target));
    }
  }

  private originalRangeContent(target: { startLine: number; endLine: number; side: 'old' | 'new' }): string {
    const rows = this.file().hunks.flatMap((h) => h.rows);
    return rows
      .filter((r) => {
        const line = target.side === 'old' ? r.oldLine : r.newLine;
        return line !== null && line >= target.startLine && line <= target.endLine;
      })
      .map((r) => (target.side === 'old' ? r.oldContent : r.newContent) ?? '')
      .join('');
  }

  protected submitComment(): void {
    const target = this.composingAt();
    const body = this.newCommentBody().trim();
    if (!target || !body) {
      return;
    }
    const suggestedContent = this.showSuggestionInput() ? this.suggestionBody() : undefined;
    this.commentAdded.emit({
      filePath: this.file().path,
      lineNumber: target.startLine,
      endLine: target.endLine !== target.startLine ? target.endLine : undefined,
      side: target.side,
      body,
      suggestedContent,
    });
    this.composingAt.set(null);
    this.newCommentBody.set('');
    this.showSuggestionInput.set(false);
    this.suggestionBody.set('');
  }

  protected isComposeRowAt(lineNumber: number, side: 'old' | 'new'): boolean {
    const target = this.composingAt();
    return target !== null && target.side === side && lineNumber === target.endLine;
  }

  protected rowClass(row: SplitDiffRow): string {
    const base = `file-diff-view__row file-diff-view__row--${row.kind}`;
    const selecting = this.isSelecting(row.oldLine ?? -1, 'old') || this.isSelecting(row.newLine ?? -1, 'new');
    return selecting ? `${base} file-diff-view__row--selecting` : base;
  }

  // Background colour alone (`rowClass`) is invisible to colourblind readers: every changed line gets a literal +/-
  // prefix, chosen per side because a 'modified' row pairs a removed old line with an added new line.
  protected oldPrefix(row: SplitDiffRow): string {
    return row.oldContent !== null && (row.kind === 'removed' || row.kind === 'modified') ? '-' : ' ';
  }

  protected newPrefix(row: SplitDiffRow): string {
    return row.newContent !== null && (row.kind === 'added' || row.kind === 'modified') ? '+' : ' ';
  }

  // Each side is tinted on its own: on a 'modified' row the old half is a removal and the new half an addition.
  // The half of an added or removed line that has no content is 'empty'.
  protected oldTone(row: SplitDiffRow): 'removed' | 'context' | 'empty' {
    if (row.oldContent === null) {
      return 'empty';
    }
    return this.oldPrefix(row) === '-' ? 'removed' : 'context';
  }

  protected newTone(row: SplitDiffRow): 'added' | 'context' | 'empty' {
    if (row.newContent === null) {
      return 'empty';
    }
    return this.newPrefix(row) === '+' ? 'added' : 'context';
  }
}
