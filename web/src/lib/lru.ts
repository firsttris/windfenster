/** A map that keeps its `capacity` most recently used entries: a read counts as a use, and the
 * entry used longest ago goes first. */
export interface Lru<K, V> {
  get(key: K): V | undefined;
  set(key: K, value: V): void;
  readonly size: number;
}

export function lru<K, V>(capacity: number): Lru<K, V> {
  const entries = new Map<K, V>();
  return {
    get(key) {
      const hit = entries.get(key);
      if (hit === undefined) return undefined;
      // Most recently used last
      entries.delete(key);
      entries.set(key, hit);
      return hit;
    },
    set(key, value) {
      entries.delete(key);
      entries.set(key, value);
      while (entries.size > capacity) entries.delete(entries.keys().next().value as K);
    },
    get size() {
      return entries.size;
    },
  };
}
