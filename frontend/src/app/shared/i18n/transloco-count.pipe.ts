import { Pipe, PipeTransform } from '@angular/core';
import { tn } from './translator';

/** `tn()` for templates: `{{ 'common.comments' | translocoCount: count }}` gives « 3 commentaires ». */
@Pipe({ name: 'translocoCount', standalone: true, pure: false })
export class TranslocoCountPipe implements PipeTransform {
  transform(key: string, count: number, params: Record<string, unknown> = {}): string {
    return tn(key, count, params);
  }
}
