import { TestBed } from '@angular/core/testing';
import { MrApprovalsPanel } from './mr-approvals-panel';
import { Review, ReviewSummary } from '../merge-requests.service';

function makeReview(overrides: Partial<Review> = {}): Review {
  return { userId: 'u1', username: 'alice', decision: 'approved', stale: false, createdAt: '2026-09-23T10:00:00', ...overrides };
}

function makeSummary(overrides: Partial<ReviewSummary> = {}): ReviewSummary {
  return { reviews: [], requiredApprovals: 2, liveApprovalCount: 0, blocked: false, ...overrides };
}

describe('MrApprovalsPanel', () => {
  function setup(inputs: { summary?: ReviewSummary | null; status?: 'open' | 'merged' | 'closed'; canWrite?: boolean }) {
    const fixture = TestBed.createComponent(MrApprovalsPanel);
    fixture.componentRef.setInput('summary', inputs.summary === undefined ? makeSummary() : inputs.summary);
    fixture.componentRef.setInput('status', inputs.status ?? 'open');
    fixture.componentRef.setInput('canWrite', inputs.canWrite ?? false);
    fixture.detectChanges();
    return fixture;
  }

  const el = (fixture: { nativeElement: HTMLElement }) => fixture.nativeElement;
  const buttonByText = (root: HTMLElement, text: string) => Array.from(root.querySelectorAll('button')).find((b) => b.textContent?.trim() === text) as HTMLButtonElement | undefined;

  it.each(['merged', 'closed'] as const)('renders nothing when the merge request is %s', (status) => {
    const fixture = setup({ status, summary: makeSummary({ blocked: true, reviews: [makeReview()] }), canWrite: true });

    expect(el(fixture).querySelector('gbt-alert')).toBeNull();
    expect(el(fixture).querySelector('li')).toBeNull();
    expect(el(fixture).querySelector('button')).toBeNull();
  });

  it('shows a warning alert when blocked by a live changes_requested review', () => {
    const fixture = setup({ summary: makeSummary({ blocked: true, reviews: [makeReview({ decision: 'changes_requested' })] }) });

    const alert = el(fixture).querySelector('gbt-alert');
    expect(alert?.textContent?.trim()).toBe('Des changements ont été demandés.');
    expect(alert?.querySelector('.gbt-alert')?.getAttribute('data-variant')).toBe('warning');
  });

  it('shows the approvals-required info alert when blocked without a live changes_requested review', () => {
    const fixture = setup({
      summary: makeSummary({ blocked: true, liveApprovalCount: 1, requiredApprovals: 2, reviews: [makeReview(), makeReview({ userId: 'u2', username: 'bob', decision: 'changes_requested', stale: true })] }),
    });

    const alert = el(fixture).querySelector('gbt-alert');
    expect(alert?.textContent?.trim()).toBe('1/2 approbations requises.');
    expect(alert?.querySelector('.gbt-alert')?.getAttribute('data-variant')).toBe('info');
  });

  it('shows no alert when the merge request is not blocked', () => {
    const fixture = setup({ summary: makeSummary({ blocked: false, reviews: [makeReview()] }) });

    expect(el(fixture).querySelector('gbt-alert')).toBeNull();
  });

  it('lists each review with its decision and flags stale ones', () => {
    const fixture = setup({
      summary: makeSummary({
        reviews: [makeReview(), makeReview({ userId: 'u2', username: 'bob', decision: 'changes_requested', stale: true })],
      }),
    });

    const items = Array.from(el(fixture).querySelectorAll('li')).map((li) => {
      const clone = li.cloneNode(true) as HTMLElement;
      clone.querySelector('gbt-avatar')?.remove();
      return (clone.textContent ?? '').replace(/\s+/g, ' ').trim();
    });
    expect(items).toEqual(['alice — a approuvé', 'bob — a demandé des changements (obsolète — nouveau commit poussé)']);
  });

  it('shows the approve and request-changes buttons only when the user can write', () => {
    const readOnly = setup({ canWrite: false });
    expect(buttonByText(el(readOnly), 'Approuver')).toBeUndefined();
    expect(buttonByText(el(readOnly), 'Demander des changements')).toBeUndefined();

    const writer = setup({ canWrite: true });
    expect(buttonByText(el(writer), 'Approuver')).toBeDefined();
    expect(buttonByText(el(writer), 'Demander des changements')).toBeDefined();
  });

  it('emits approve and requestChanges when the buttons are clicked', () => {
    const fixture = setup({ canWrite: true });
    const approve = vi.fn();
    const requestChanges = vi.fn();
    fixture.componentInstance.approve.subscribe(approve);
    fixture.componentInstance.requestChanges.subscribe(requestChanges);

    buttonByText(el(fixture), 'Approuver')!.click();
    expect(approve).toHaveBeenCalledTimes(1);
    expect(requestChanges).not.toHaveBeenCalled();

    buttonByText(el(fixture), 'Demander des changements')!.click();
    expect(requestChanges).toHaveBeenCalledTimes(1);
  });

  it('has no heading of its own: the page panel that hosts it names it', () => {
    const fixture = setup({ canWrite: true, summary: makeSummary({ reviews: [makeReview()] }) });

    expect(el(fixture).querySelector('h1, h2, h3, h4')).toBeNull();
    expect(el(fixture).querySelector('.mr-approvals')).toBeTruthy();
  });

  it('puts the status first, then the reviews, then the actions', () => {
    const fixture = setup({ canWrite: true, summary: makeSummary({ blocked: true, liveApprovalCount: 0, requiredApprovals: 1, reviews: [makeReview()] }) });

    const alert = el(fixture).querySelector('gbt-alert')!;
    const list = el(fixture).querySelector('ul')!;
    const actions = el(fixture).querySelector('.mr-approvals__actions')!;
    expect(alert.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(list.compareDocumentPosition(actions) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(actions.querySelectorAll('button').length).toBe(2);
  });

  it('marks each decision with an icon and a text, never colour alone', () => {
    const fixture = setup({ summary: makeSummary({ reviews: [makeReview(), makeReview({ userId: 'u2', username: 'bob', decision: 'changes_requested' })] }) });

    const markers = Array.from(el(fixture).querySelectorAll('.mr-approvals__decision-icon'));
    expect(markers.map((marker) => marker.getAttribute('data-decision'))).toEqual(['approved', 'changes_requested']);
    const discs = markers.map((marker) => marker.querySelector('.gbt-icon-marker')!);
    expect(discs.map((disc) => disc.getAttribute('data-tone'))).toEqual(['success', 'error']);
    expect(discs.every((disc) => disc.getAttribute('aria-hidden') === 'true')).toBe(true);
  });

  it('copes with a null summary (not loaded yet)', () => {
    const fixture = setup({ summary: null, canWrite: true });

    expect(el(fixture).querySelector('gbt-alert')).toBeNull();
    expect(el(fixture).querySelector('li')?.textContent?.trim()).toBe("Aucune revue pour l'instant.");
  });
});
