/** A copy of `set` with `item` added, or removed if it was there. */
export function toggled<T>(set: ReadonlySet<T>, item: T): ReadonlySet<T> {
  const next = new Set(set);
  if (!next.delete(item)) next.add(item);
  return next;
}
