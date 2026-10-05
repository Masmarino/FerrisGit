import { Component, computed, input, output } from '@angular/core';
import { Alert, AlertVariant } from '@masmarino/gabarit/alert';
import { Avatar } from '@masmarino/gabarit/avatar';
import { Button } from '@masmarino/gabarit/button';
import { IconMarker } from '@masmarino/gabarit/icon-marker';
import { ReviewSummary } from '../merge-requests.service';
import { reviewStatus } from '../review-status';

@Component({
  selector: 'fg-mr-approvals-panel',
  standalone: true,
  imports: [Alert, Avatar, Button, IconMarker],
  templateUrl: './mr-approvals-panel.html',
  styleUrl: './mr-approvals-panel.scss',
})
export class MrApprovalsPanel {
  summary = input<ReviewSummary | null>(null);
  status = input.required<'open' | 'merged' | 'closed'>();
  canWrite = input(false);

  approve = output<void>();
  requestChanges = output<void>();

  protected alert = computed<{ variant: AlertVariant; text: string } | null>(() => {
    const status = reviewStatus(this.summary());
    return status?.blocked ? { variant: status.tone === 'warning' ? 'warning' : 'info', text: status.text } : null;
  });
}
