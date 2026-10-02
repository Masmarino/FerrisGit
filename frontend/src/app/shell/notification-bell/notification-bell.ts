import { Component, OnDestroy, OnInit, inject, signal, viewChild } from '@angular/core';
import { Router } from '@angular/router';
import { Notification, NotificationsService } from '../../notifications/notifications.service';
import { notificationLink, notificationQueryParams, notificationSentence } from '../../notifications/notification-display';
import { Button, NotificationDot, Popover } from '@masmarino/gabarit';

const POLL_INTERVAL_MS = 20000;

@Component({
  selector: 'fg-notification-bell',
  standalone: true,
  imports: [Button, Popover, NotificationDot],
  templateUrl: './notification-bell.html',
  styleUrl: './notification-bell.scss',
})
export class NotificationBell implements OnInit, OnDestroy {
  private notifications = inject(NotificationsService);
  private router = inject(Router);
  private pollHandle: ReturnType<typeof setInterval> | null = null;
  private popover = viewChild(Popover);

  protected unreadCount = signal(0);
  protected items = signal<Notification[]>([]);

  ngOnInit(): void {
    this.refreshCount();
    this.pollHandle = setInterval(() => this.refreshCount(), POLL_INTERVAL_MS);
  }

  ngOnDestroy(): void {
    if (this.pollHandle !== null) {
      clearInterval(this.pollHandle);
    }
  }

  private refreshCount(): void {
    this.notifications.unreadCount().subscribe({
      next: (result) => this.unreadCount.set(result.count),
      error: () => {},
    });
  }

  // Bound on the trigger button, not the popover host, so it runs before the popover's own toggle handler. `open()`
  // still holds the state from before the click, so the list only refreshes when the click opens it.
  protected onTriggerClick(): void {
    if (!this.popover()?.open()) {
      this.notifications.list().subscribe({ next: (list) => this.items.set(list) });
    }
  }

  protected sentence(n: Notification): string {
    return notificationSentence(n);
  }

  select(n: Notification): void {
    this.notifications.markRead(n.id).subscribe({
      next: () => {
        this.items.update((items) => items.map((item) => (item.id === n.id ? { ...item, read: true } : item)));
        this.refreshCount();
      },
    });
    this.popover()?.close();
    const queryParams = notificationQueryParams(n);
    void (queryParams ? this.router.navigate(notificationLink(n), { queryParams }) : this.router.navigate(notificationLink(n)));
  }

  markAllRead(): void {
    this.notifications.markAllRead().subscribe({
      next: () => {
        this.items.update((items) => items.map((item) => ({ ...item, read: true })));
        this.unreadCount.set(0);
      },
    });
  }
}
