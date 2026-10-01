import { TestBed } from '@angular/core/testing';
import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter } from '@angular/router';
import { AuthService } from './auth.service';
import { authInterceptor } from './auth.interceptor';

const TOKEN_KEY = 'ferrisgit_token';

describe('authInterceptor', () => {
  afterEach(() => {
    localStorage.removeItem(TOKEN_KEY);
  });

  it('attaches Authorization: Bearer <token> to outgoing requests when a token is present', () => {
    localStorage.setItem(TOKEN_KEY, 'a-token');
    TestBed.configureTestingModule({
      providers: [provideHttpClient(withInterceptors([authInterceptor])), provideHttpClientTesting(), provideRouter([])],
    });
    const http = TestBed.inject(HttpClient);
    const httpTesting = TestBed.inject(HttpTestingController);

    http.get('/api/repositories').subscribe();
    const req = httpTesting.expectOne('/api/repositories');

    expect(req.request.headers.get('Authorization')).toBe('Bearer a-token');
    req.flush([]);
  });

  it('logs out and navigates to /login on a 401 response', () => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(withInterceptors([authInterceptor])), provideHttpClientTesting(), provideRouter([])],
    });
    const auth = TestBed.inject(AuthService);
    const router = TestBed.inject(Router);
    const http = TestBed.inject(HttpClient);
    const httpTesting = TestBed.inject(HttpTestingController);
    const logoutSpy = vi.spyOn(auth, 'logout');
    const navigateSpy = vi.spyOn(router, 'navigateByUrl');

    http.get('/api/repositories').subscribe({ error: () => {} });
    httpTesting.expectOne('/api/repositories').flush('unauthorized', { status: 401, statusText: 'Unauthorized' });

    expect(logoutSpy).toHaveBeenCalled();
    expect(navigateSpy).toHaveBeenCalledWith('/login');
  });

  describe('a 401 and the token it was sent with', () => {
    function setup() {
      TestBed.configureTestingModule({
        providers: [provideHttpClient(withInterceptors([authInterceptor])), provideHttpClientTesting(), provideRouter([])],
      });
      const auth = TestBed.inject(AuthService);
      const router = TestBed.inject(Router);
      return {
        auth,
        http: TestBed.inject(HttpClient),
        httpTesting: TestBed.inject(HttpTestingController),
        logoutSpy: vi.spyOn(auth, 'logout'),
        navigateSpy: vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true),
      };
    }
    const unauthorized = { status: 401, statusText: 'Unauthorized' };

    it('keeps a newer session when the 401 answers a request sent with an older token', () => {
      localStorage.setItem(TOKEN_KEY, 'old');
      const { auth, http, httpTesting, logoutSpy, navigateSpy } = setup();

      http.get('/api/notifications').subscribe({ error: () => {} });
      const late = httpTesting.expectOne('/api/notifications');
      expect(late.request.headers.get('Authorization')).toBe('Bearer old');
      auth.setToken('new'); // for example, the password change handing back a fresh session
      late.flush('unauthorized', unauthorized);

      expect(logoutSpy).not.toHaveBeenCalled();
      expect(navigateSpy).not.toHaveBeenCalled();
      expect(auth.token()).toBe('new');
      expect(localStorage.getItem(TOKEN_KEY)).toBe('new');
    });

    it('still logs out and navigates on a 401 for a request sent with the current token', () => {
      localStorage.setItem(TOKEN_KEY, 'current');
      const { auth, http, httpTesting, logoutSpy, navigateSpy } = setup();

      http.get('/api/repositories').subscribe({ error: () => {} });
      httpTesting.expectOne('/api/repositories').flush('unauthorized', unauthorized);

      expect(logoutSpy).toHaveBeenCalledTimes(1);
      expect(navigateSpy).toHaveBeenCalledWith('/login');
      expect(auth.token()).toBeNull();
    });

    it('is unchanged for a request sent without a token and still without one: logs out and navigates', () => {
      const { http, httpTesting, logoutSpy, navigateSpy } = setup();

      http.post('/api/auth/login', {}).subscribe({ error: () => {} });
      const req = httpTesting.expectOne('/api/auth/login');
      expect(req.request.headers.has('Authorization')).toBe(false);
      req.flush('unauthorized', unauthorized);

      expect(logoutSpy).toHaveBeenCalledTimes(1);
      expect(navigateSpy).toHaveBeenCalledWith('/login');
    });

    it('keeps a session started after a token-less request was sent', () => {
      const { auth, http, httpTesting, logoutSpy } = setup();

      http.get('/api/repositories').subscribe({ error: () => {} });
      const req = httpTesting.expectOne('/api/repositories');
      auth.setToken('just-signed-in');
      req.flush('unauthorized', unauthorized);

      expect(logoutSpy).not.toHaveBeenCalled();
      expect(auth.token()).toBe('just-signed-in');
    });

    it('still passes the error on to the caller when it keeps the session', () => {
      localStorage.setItem(TOKEN_KEY, 'old');
      const { auth, http, httpTesting } = setup();
      const errors: number[] = [];

      http.get('/api/notifications').subscribe({ error: (e) => errors.push(e.status) });
      const late = httpTesting.expectOne('/api/notifications');
      auth.setToken('new');
      late.flush('unauthorized', unauthorized);

      expect(errors).toEqual([401]);
    });
  });
});
