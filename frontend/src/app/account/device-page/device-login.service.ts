import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import type { PasskeyChallenge } from '@masmarino/gabarit/auth';

export interface DeviceRequest {
  userCode: string;
  /** Comes from the terminal and nothing vouches for it: text only, and never trusted. */
  clientName: string;
  scopes: string[];
  tokenLifetimeDays: number;
  requestedAt: string;
  expiresAt: string;
  hasTotp: boolean;
  hasPasskey: boolean;
}

export type DeviceProof = { code: string } | { backupCode: string } | { challengeId: string; credential: unknown };

/** The web side of `fgit auth login`. A refused second factor answers 400, not 401, so the interceptor keeps the session. */
@Injectable({ providedIn: 'root' })
export class DeviceLoginService {
  private http = inject(HttpClient);

  find(userCode: string) {
    return this.http.get<DeviceRequest>(this.url(userCode));
  }

  startPasskey(userCode: string) {
    return this.http.post<PasskeyChallenge>(`${this.url(userCode)}/passkey`, null);
  }

  approve(userCode: string, proof: DeviceProof) {
    return this.http.post<void>(`${this.url(userCode)}/approve`, proof);
  }

  deny(userCode: string) {
    return this.http.post<void>(`${this.url(userCode)}/deny`, null);
  }

  private url(userCode: string): string {
    return `/api/auth/device/${encodeURIComponent(userCode)}`;
  }
}
