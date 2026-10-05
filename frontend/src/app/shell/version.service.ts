import { HttpClient } from '@angular/common/http';
import { inject, Injectable, signal } from '@angular/core';

/** The running release (`GET /api/version`), shown at the foot of the navigation. Asked once; a failure asks again next time. */
@Injectable({ providedIn: 'root' })
export class VersionService {
  private http = inject(HttpClient);
  readonly version = signal<string | null>(null);
  private loading = false;

  load(): void {
    if (this.loading || this.version() !== null) {
      return;
    }
    this.loading = true;
    this.http.get<{ version: string }>('/api/version').subscribe({
      next: (res) => {
        this.loading = false;
        this.version.set(res.version);
      },
      error: () => (this.loading = false),
    });
  }
}
