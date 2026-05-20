/**
 * Coalesce concurrent GETs for the same path so poll timers do not stack Pending requests.
 */
const pollInFlight = new Map<string, Promise<unknown>>();

export async function coalescePollFetch<T>(
  key: string,
  run: () => Promise<T>,
): Promise<T> {
  const existing = pollInFlight.get(key);
  if (existing) {
    return existing as Promise<T>;
  }
  const promise = run().finally(() => {
    if (pollInFlight.get(key) === promise) {
      pollInFlight.delete(key);
    }
  });
  pollInFlight.set(key, promise);
  return promise;
}
