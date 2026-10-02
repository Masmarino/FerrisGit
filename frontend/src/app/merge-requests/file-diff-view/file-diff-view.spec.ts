import { TestBed } from '@angular/core/testing';
import { FileDiffView } from './file-diff-view';
import { FileDiff } from '../merge-requests.service';

describe('FileDiffView', () => {
  function setup(file: FileDiff) {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(FileDiffView);
    fixture.componentRef.setInput('file', file);
    fixture.componentRef.setInput('comments', []);
    return fixture;
  }

  it('renders one row per split diff row, with old and new content in separate cells', () => {
    const file: FileDiff = {
      path: 'README.md',
      change: 'modified',
      hunks: [{ rows: [{ oldLine: 1, oldContent: 'old\n', newLine: 1, newContent: 'new\n', kind: 'modified' }] }],
    };
    const fixture = setup(file);
    fixture.detectChanges();

    const cells = fixture.nativeElement.querySelectorAll('td');
    const text = Array.from(cells as NodeListOf<HTMLElement>).map((c) => c.textContent?.trim());
    // Both sides get a literal +/- prefix, so colour isn't the only signal.
    expect(text).toContain('-old');
    expect(text).toContain('+new');
  });

  it('renders every hunk in one table (one tbody per hunk), with its hidden column headers inside the th', () => {
    const file: FileDiff = {
      path: 'src/lib.rs',
      change: 'modified',
      hunks: [
        { rows: [{ oldLine: 1, oldContent: 'a\n', newLine: 1, newContent: 'a\n', kind: 'context' }] },
        { rows: [{ oldLine: 40, oldContent: 'b\n', newLine: 40, newContent: 'c\n', kind: 'modified' }] },
      ],
    };
    const fixture = setup(file);
    fixture.detectChanges();

    const tables = fixture.nativeElement.querySelectorAll('table');
    expect(tables.length, 'one table, so every hunk shares the same columns').toBe(1);
    expect(tables[0].tBodies.length).toBe(2);
    expect(tables[0].querySelectorAll('col').length).toBe(4);
    const headers: HTMLTableCellElement[] = Array.from(tables[0].querySelectorAll('thead th'));
    expect(headers.map((th) => th.textContent?.trim())).toEqual(['Ligne (ancienne version)', 'Ancienne version', 'Ligne (nouvelle version)', 'Nouvelle version']);
    // A visually hidden th would leave the fixed-layout grid, so the hidden text is a span inside it.
    expect(headers.every((th) => !th.classList.contains('sr-only') && th.querySelector('span.sr-only') !== null)).toBe(true);
  });

  it('marks the unchanged lines hidden above a hunk with a hunk header, and none above a hunk starting at line 1', () => {
    const file: FileDiff = {
      path: 'src/lib.rs',
      change: 'modified',
      hunks: [
        { rows: [{ oldLine: 1, oldContent: 'a\n', newLine: 1, newContent: 'a\n', kind: 'context' }] },
        {
          rows: [
            { oldLine: 41, oldContent: 'b\n', newLine: 42, newContent: 'b\n', kind: 'context' },
            { oldLine: 42, oldContent: 'c\n', newLine: null, newContent: null, kind: 'removed' },
          ],
        },
      ],
    };
    const fixture = setup(file);
    fixture.detectChanges();

    const headers: HTMLElement[] = Array.from(fixture.nativeElement.querySelectorAll('.file-diff-view__hunk-header'));
    expect(headers.map((header) => header.textContent?.trim())).toEqual(['@@ -41,2 +42,1 @@']);
    expect(fixture.nativeElement.querySelectorAll('tbody')[1].firstElementChild.classList).toContain('file-diff-view__hunk-header');
  });

  it('shows only the new side of an added file, as one gutter and one code column', () => {
    const file: FileDiff = {
      path: 'src/new.rs',
      change: 'added',
      hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 1, newContent: 'fn main() {}\n', kind: 'added' }] }],
    };
    const fixture = setup(file);
    fixture.detectChanges();

    const table: HTMLTableElement = fixture.nativeElement.querySelector('table');
    expect(table.querySelectorAll('col').length).toBe(2);
    expect(Array.from(table.querySelectorAll('thead th')).map((th) => th.textContent?.trim())).toEqual(['Ligne (nouvelle version)', 'Nouvelle version']);
    const cells: HTMLTableCellElement[] = Array.from(table.querySelectorAll('tbody tr')[0].querySelectorAll('td'));
    expect(cells.length).toBe(2);
    expect(cells[0].querySelector('button')?.getAttribute('aria-label')).toBe('Commenter la ligne 1 (nouvelle version)');
    expect(cells[1].textContent?.trim()).toBe('+fn main() {}');
    expect(table.querySelector('.file-diff-view__content--empty'), 'no dead, empty half').toBeNull();
  });

  it('shows only the old side of a deleted file, and full-width rows span its two columns', () => {
    const file: FileDiff = {
      path: 'src/old.rs',
      change: 'deleted',
      hunks: [{ rows: [{ oldLine: 1, oldContent: 'gone\n', newLine: null, newContent: null, kind: 'removed' }] }],
    };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'why remove this?', createdAt: '2026-01-01', replyToId: null, filePath: 'src/old.rs', lineNumber: 1, side: 'old', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    const table: HTMLTableElement = fixture.nativeElement.querySelector('table');
    expect(Array.from(table.querySelectorAll('thead th')).map((th) => th.textContent?.trim())).toEqual(['Ligne (ancienne version)', 'Ancienne version']);
    expect(table.querySelector('.file-diff-view__row')!.querySelectorAll('td').length).toBe(2);
    expect(table.querySelector('.file-diff-view__thread-row > td')!.getAttribute('colspan')).toBe('2');
  });

  it('tints each half on its own: a modified line is a removal on the old side and an addition on the new side', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [
      { oldLine: 1, oldContent: 'old\n', newLine: 1, newContent: 'new\n', kind: 'modified' },
      { oldLine: null, oldContent: null, newLine: 2, newContent: 'more\n', kind: 'added' },
    ] }] };
    const fixture = setup(file);
    fixture.detectChanges();

    const [modified, added] = Array.from(fixture.nativeElement.querySelectorAll('.file-diff-view__row') as NodeListOf<HTMLTableRowElement>);
    const contents = (row: HTMLTableRowElement) => Array.from(row.querySelectorAll('.file-diff-view__content'));
    expect(contents(modified)[0].classList).toContain('file-diff-view__content--removed');
    expect(contents(modified)[1].classList).toContain('file-diff-view__content--added');
    expect(contents(added)[0].classList).toContain('file-diff-view__content--empty');
    expect(contents(added)[1].classList).toContain('file-diff-view__content--added');
  });

  it('shows a comment affordance only on a side that has content', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 1, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.detectChanges();

    const addButtons = fixture.nativeElement.querySelectorAll('.file-diff-view__add-comment');
    expect(addButtons.length).toBe(1);
  });

  it('groups comments into a thread under the row they are anchored to', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'why?', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
      { id: 'reply', authorId: 'u2', body: 'because', createdAt: '2026-01-01', replyToId: 'root', filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('why?');
    expect(text).toContain('because');
  });

  // Two roots can anchor to the same file, line and side: every thread must render, not just the last.
  it('renders every independent thread anchored to the same line, not just the last one', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root-a', authorId: 'u1', body: 'first reviewer concern', createdAt: '2026-01-01T00:00:00Z', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
      { id: 'root-b', authorId: 'u2', body: 'second reviewer concern', createdAt: '2026-01-01T00:00:01Z', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('first reviewer concern');
    expect(text).toContain('second reviewer concern');
  });

  it('renders a thread anchored to the old side of the diff, not just the new side', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: 2, oldContent: 'removed line\n', newLine: null, newContent: null, kind: 'removed' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'why remove this?', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'old', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('why remove this?');
  });

  it('shows an outdated badge when the thread root is outdated', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'why?', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: true, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    const badges = Array.from(fixture.nativeElement.querySelectorAll('gbt-badge') as NodeListOf<HTMLElement>).map((badge) => badge.textContent?.trim());
    expect(badges).toEqual(['Périmé']);
  });

  it('shows a resolved badge on an expanded resolved thread, and no outdated badge on it', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'settled now', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: true, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('gbt-badge')).toBeNull();

    (fixture.nativeElement.querySelector('.file-diff-view__thread-summary button') as HTMLButtonElement).click();
    fixture.detectChanges();

    const badges = Array.from(fixture.nativeElement.querySelectorAll('gbt-badge') as NodeListOf<HTMLElement>).map((badge) => badge.textContent?.trim());
    expect(badges).toEqual(['Résolu']);
  });

  // gbt-textarea writes a cleared value back into its view in a microtask, and the app is zoneless, so
  // wait for whenStable before reading the field.
  it('clears the reply draft after submitting, even though the thread row is never torn down', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'why?', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    const textarea: HTMLTextAreaElement = fixture.nativeElement.querySelector('textarea[placeholder="Répondre"]');
    textarea.value = 'because';
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.componentInstance['replyBodyFor']('root')).toBe('because');

    const replyButton: HTMLButtonElement = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Répondre') as HTMLButtonElement;
    replyButton.click();
    fixture.detectChanges();

    expect(fixture.componentInstance['replyBodyFor']('root')).toBe('');
  });

  it('empties the reply box itself once the reply is sent (the thread row is never torn down)', async () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'why?', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();
    let emitted: { replyToId: string; body: string } | undefined;
    fixture.componentInstance.replyAdded.subscribe((e) => (emitted = e));

    const textarea: HTMLTextAreaElement = fixture.nativeElement.querySelector('textarea[placeholder="Répondre"]');
    textarea.value = 'because';
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const replyButton = Array.from(fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>).find((b) => b.textContent?.trim() === 'Répondre')!;
    replyButton.click();
    // NgModel pushes the cleared draft into the field on a microtask.
    await fixture.whenStable();
    fixture.detectChanges();

    expect(emitted).toEqual({ replyToId: 'root', body: 'because' });
    expect(textarea.value).toBe('');
  });

  it('keeps each thread\'s reply draft independent — typing in one does not leak into another', () => {
    const file: FileDiff = {
      path: 'README.md',
      change: 'modified',
      hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'a\n', kind: 'added' }, { oldLine: null, oldContent: null, newLine: 5, newContent: 'b\n', kind: 'added' }] }],
    };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root-1', authorId: 'u1', body: 'first thread', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
      { id: 'root-2', authorId: 'u1', body: 'second thread', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 5, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    const textareas: HTMLTextAreaElement[] = Array.from(fixture.nativeElement.querySelectorAll('textarea[placeholder="Répondre"]'));
    expect(textareas.length).toBe(2);

    textareas[0].value = 'draft for the first thread only';
    textareas[0].dispatchEvent(new Event('input'));
    fixture.detectChanges();

    expect(textareas[0].value).toBe('draft for the first thread only');
    expect(textareas[1].value).toBe('');
  });

  it('emits resolveToggled with resolved:true when Résoudre is clicked on an open thread', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'why?', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: false, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();
    let emitted: { commentId: string; resolved: boolean } | undefined;
    fixture.componentInstance.resolveToggled.subscribe((e) => (emitted = e));

    const resolveButton: HTMLButtonElement = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Résoudre') as HTMLButtonElement;
    resolveButton.click();

    expect(emitted).toEqual({ commentId: 'root', resolved: true });
  });

  it('collapses a resolved thread to a summary line, and expanding it reveals the body again', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'new\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'settled now', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, side: 'new', outdated: false, resolved: true, endLine: null, suggestedContent: null, appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).not.toContain('settled now');
    const summaryButton: HTMLButtonElement = fixture.nativeElement.querySelector('.file-diff-view__thread-summary button');
    expect(summaryButton).not.toBeNull();
    expect(summaryButton.textContent?.trim()).toBe('Résolu — afficher');

    summaryButton.click();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('settled now');
    expect(fixture.nativeElement.querySelector('.file-diff-view__thread-summary')).toBeNull();
    const reopenButton: HTMLButtonElement = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Rouvrir') as HTMLButtonElement;
    expect(reopenButton).toBeTruthy();
  });

  it('dragging from one gutter row to another opens the composer for that whole range', () => {
    const file: FileDiff = {
      path: 'README.md',
      change: 'modified',
      hunks: [{ rows: [
        { oldLine: null, oldContent: null, newLine: 1, newContent: 'a\n', kind: 'added' },
        { oldLine: null, oldContent: null, newLine: 2, newContent: 'b\n', kind: 'added' },
        { oldLine: null, oldContent: null, newLine: 3, newContent: 'c\n', kind: 'added' },
      ] }],
    };
    const fixture = setup(file);
    fixture.detectChanges();

    const addButtons: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('.file-diff-view__add-comment'));
    addButtons[0].dispatchEvent(new MouseEvent('mousedown'));
    addButtons[1].dispatchEvent(new MouseEvent('mouseover'));
    addButtons[2].dispatchEvent(new MouseEvent('mouseover'));
    fixture.componentInstance.finishSelecting();
    fixture.detectChanges();

    const composeTextareas = fixture.nativeElement.querySelectorAll('textarea[placeholder="Laisser un commentaire"]');
    expect(composeTextareas.length, 'the composer must render once for the whole range, not once per row').toBe(1);
  });

  it('emits endLine and suggestedContent when a suggestion is submitted for a multi-line selection', () => {
    const file: FileDiff = {
      path: 'README.md',
      change: 'modified',
      hunks: [{ rows: [
        { oldLine: null, oldContent: null, newLine: 1, newContent: 'a\n', kind: 'added' },
        { oldLine: null, oldContent: null, newLine: 2, newContent: 'b\n', kind: 'added' },
      ] }],
    };
    const fixture = setup(file);
    fixture.detectChanges();

    const addButtons: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('.file-diff-view__add-comment'));
    addButtons[0].dispatchEvent(new MouseEvent('mousedown'));
    addButtons[1].dispatchEvent(new MouseEvent('mouseover'));
    fixture.componentInstance.finishSelecting();
    fixture.detectChanges();

    let emitted: { filePath: string; lineNumber: number; endLine?: number; side: 'old' | 'new'; body: string; suggestedContent?: string } | undefined;
    fixture.componentInstance.commentAdded.subscribe((e) => (emitted = e));

    const commentBody: HTMLTextAreaElement = fixture.nativeElement.querySelector('textarea[placeholder="Laisser un commentaire"]');
    commentBody.value = 'swap this';
    commentBody.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const suggestionToggle: HTMLButtonElement = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Proposer un remplacement') as HTMLButtonElement;
    suggestionToggle.click();
    fixture.detectChanges();

    expect(fixture.componentInstance['suggestionBody'](), "the suggestion draft must pre-fill with the range's own original content").toBe('a\nb\n');

    const suggestionBody: HTMLTextAreaElement = fixture.nativeElement.querySelector('textarea[placeholder="Contenu de remplacement"]');
    suggestionBody.value = 'x\ny\n';
    suggestionBody.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const commentButton: HTMLButtonElement = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Commenter') as HTMLButtonElement;
    commentButton.click();

    expect(emitted).toEqual({ filePath: 'README.md', lineNumber: 1, endLine: 2, side: 'new', body: 'swap this', suggestedContent: 'x\ny\n' });
  });

  it('does not offer to propose a replacement when the composer is opened on the old side of the diff', () => {
    const file: FileDiff = {
      path: 'README.md',
      change: 'modified',
      hunks: [{ rows: [{ oldLine: 1, oldContent: 'old\n', newLine: null, newContent: null, kind: 'removed' }] }],
    };
    const fixture = setup(file);
    fixture.detectChanges();

    const addButtons: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('.file-diff-view__add-comment'));
    expect(addButtons.length, 'the old-side "+" button must exist to interact with').toBe(1);
    addButtons[0].dispatchEvent(new MouseEvent('mousedown'));
    fixture.componentInstance.finishSelecting();
    fixture.detectChanges();

    const composeTextarea = fixture.nativeElement.querySelector('textarea[placeholder="Laisser un commentaire"]');
    expect(composeTextarea, 'a plain comment must still be composable on the old side').not.toBeNull();
    const suggestionToggle = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Proposer un remplacement');
    expect(suggestionToggle, 'the old side cannot anchor a suggestion, so the toggle must not appear').toBeUndefined();
  });

  it('renders a pending suggestion with an apply button, keyed to the range end line', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [
      { oldLine: null, oldContent: null, newLine: 2, newContent: 'b\n', kind: 'added' },
      { oldLine: null, oldContent: null, newLine: 3, newContent: 'c\n', kind: 'added' },
    ] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('canWrite', true);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'swap this', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, endLine: 3, side: 'new', outdated: false, resolved: false, suggestedContent: 'x\ny\n', appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('x');
    expect(fixture.nativeElement.textContent).toContain('y');
    let emitted: { commentId: string } | undefined;
    fixture.componentInstance.applySuggestionClicked.subscribe((e) => (emitted = e));
    const applyButton: HTMLButtonElement = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) => (b as HTMLButtonElement).textContent?.trim() === 'Appliquer la suggestion') as HTMLButtonElement;
    applyButton.click();

    expect(emitted).toEqual({ commentId: 'root' });
  });

  it('does not render the apply button for a pending suggestion when the viewer has no write access', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'b\n', kind: 'added' }] }] };
    const fixture = setup(file);
    // canWrite is left unset on purpose: it must default to false.
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'swap this', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, endLine: null, side: 'new', outdated: false, resolved: false, suggestedContent: 'x\n', appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('x');
    expect(Array.from(fixture.nativeElement.querySelectorAll('button')).some((b) => (b as HTMLButtonElement).textContent?.trim() === 'Appliquer la suggestion')).toBe(false);
  });

  it('renders the apply button for a pending suggestion when the viewer has write access', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'b\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('canWrite', true);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'swap this', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, endLine: null, side: 'new', outdated: false, resolved: false, suggestedContent: 'x\n', appliedAt: null, appliedCommitSha: null },
    ]);
    fixture.detectChanges();

    expect(Array.from(fixture.nativeElement.querySelectorAll('button')).some((b) => (b as HTMLButtonElement).textContent?.trim() === 'Appliquer la suggestion')).toBe(true);
  });

  it('shows the applied commit sha instead of an apply button once a suggestion is applied', () => {
    const file: FileDiff = { path: 'README.md', change: 'modified', hunks: [{ rows: [{ oldLine: null, oldContent: null, newLine: 2, newContent: 'b\n', kind: 'added' }] }] };
    const fixture = setup(file);
    fixture.componentRef.setInput('comments', [
      { id: 'root', authorId: 'u1', body: 'swap this', createdAt: '2026-01-01', replyToId: null, filePath: 'README.md', lineNumber: 2, endLine: null, side: 'new', outdated: false, resolved: false, suggestedContent: 'x\n', appliedAt: '2026-01-02', appliedCommitSha: 'deadbeef' },
    ]);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('deadbeef');
    expect(Array.from(fixture.nativeElement.querySelectorAll('button')).some((b) => (b as HTMLButtonElement).textContent?.trim() === 'Appliquer la suggestion')).toBe(false);
  });
});
