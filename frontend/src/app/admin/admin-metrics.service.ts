import { HttpClient, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface AdminStats {
  totalUsers: number;
  totalRepositories: number;
  pipelinesLast7Days: number;
}

export interface MetricsSnapshot {
  recordedAt: string;
  totalUsers: number;
  totalRepositories: number;
  totalStorageBytes: number;
}

export interface ComponentHealth {
  status: 'up' | 'down';
  detail: string | null;
}

export interface DatabaseHealth extends ComponentHealth {
  responseTimeMs: number;
  activeConnections: number;
  maxConnections: number;
  serverVersion: string | null;
}

export interface StorageHealth extends ComponentHealth {
  usedBytes: number;
  freeBytes: number;
  totalBytes: number;
}

export interface HealthStatus {
  database: DatabaseHealth;
  storage: StorageHealth;
  uptimeSeconds: number;
}

@Injectable({ providedIn: 'root' })
export class AdminMetricsService {
  private http = inject(HttpClient);

  getStats() {
    return this.http.get<AdminStats>('/api/admin/stats');
  }

  getHistory(days: number) {
    return this.http.get<MetricsSnapshot[]>('/api/admin/metrics/history', { params: new HttpParams().set('days', days) });
  }

  getHealth() {
    return this.http.get<HealthStatus>('/api/admin/health');
  }
}
