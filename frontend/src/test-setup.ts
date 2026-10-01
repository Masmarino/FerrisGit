/**
 * Under this test runner, jsdom's `localStorage` is a bare object without the Storage methods, so field initializers
 * that read it (like AuthService's token signal) throw. In that case, install a minimal in-memory Storage.
 */
function createStorage(): Storage {
  const entries = new Map<string, string>()
  return {
    get length(): number {
      return entries.size
    },
    clear: (): void => entries.clear(),
    getItem: (key: string): string | null => entries.get(key) ?? null,
    key: (index: number): string | null => [...entries.keys()][index] ?? null,
    removeItem: (key: string): void => {
      entries.delete(key)
    },
    setItem: (key: string, value: string): void => {
      entries.set(key, String(value))
    },
  } as Storage
}

for (const name of ['localStorage', 'sessionStorage'] as const) {
  const existing = (globalThis as Record<string, unknown>)[name] as Storage | undefined
  if (typeof existing?.getItem !== 'function') {
    Object.defineProperty(globalThis, name, { value: createStorage(), configurable: true })
  }
}
