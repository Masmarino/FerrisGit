import { ApplicationConfig, LOCALE_ID, provideZonelessChangeDetection } from '@angular/core';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { provideRouter } from '@angular/router';
import { authInterceptor } from './auth/auth.interceptor';
import { provideFerrisgitAuth } from './auth/auth-kit';
import { routes } from './app.routes';
import { provideFerrisgitIcons } from './shared/register-icons';

export const appConfig: ApplicationConfig = {
  providers: [
    provideZonelessChangeDetection(),
    provideRouter(routes),
    provideHttpClient(withInterceptors([authInterceptor])),
    provideFerrisgitIcons(),
    provideFerrisgitAuth(),
    { provide: LOCALE_ID, useValue: 'fr' },
  ],
};
