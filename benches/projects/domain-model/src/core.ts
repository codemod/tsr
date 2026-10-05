// Shared building blocks for every generated domain module.

export type Id<Brand extends string> = string & { readonly __brand: Brand };

export interface Entity<Brand extends string> {
  readonly id: Id<Brand>;
  readonly createdAt: number;
}

export type Result<T, E = string> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

export function ok<T>(value: T): Result<T, never> {
  return { ok: true, value };
}

export function fail<E>(error: E): Result<never, E> {
  return { ok: false, error };
}

export function unwrapOr<T, E>(result: Result<T, E>, fallback: T): T {
  return result.ok ? result.value : fallback;
}

export function mapResult<T, U, E>(result: Result<T, E>, map: (value: T) => U): Result<U, E> {
  if (result.ok) {
    return ok(map(result.value));
  }
  return result;
}

export type DeepReadonly<T> = T extends (infer U)[]
  ? ReadonlyArray<DeepReadonly<U>>
  : T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;

export type Patch<T> = { [K in keyof T]?: T[K] extends object ? Patch<T[K]> : T[K] };

export type KeysOfType<T, V> = { [K in keyof T]-?: T[K] extends V ? K : never }[keyof T];

export interface Repository<T extends Entity<B>, B extends string> {
  get(id: Id<B>): T | undefined;
  list(filter?: (item: T) => boolean): T[];
  save(item: T): Result<T>;
  remove(id: Id<B>): boolean;
}

export class MemoryRepository<T extends Entity<B>, B extends string> implements Repository<T, B> {
  private readonly items = new Map<Id<B>, T>();

  get(id: Id<B>): T | undefined {
    return this.items.get(id);
  }

  list(filter?: (item: T) => boolean): T[] {
    const all = Array.from(this.items.values());
    return filter ? all.filter(filter) : all;
  }

  save(item: T): Result<T> {
    if (item.id.length === 0) {
      return fail("empty id");
    }
    this.items.set(item.id, item);
    return ok(item);
  }

  remove(id: Id<B>): boolean {
    return this.items.delete(id);
  }

  get size(): number {
    return this.items.size;
  }
}

export abstract class Service<T extends Entity<B>, B extends string> {
  protected constructor(protected readonly repository: Repository<T, B>) {}

  abstract validate(item: T): string[];

  create(item: T): Result<T, string[]> {
    const problems = this.validate(item);
    if (problems.length > 0) {
      return fail(problems);
    }
    const saved = this.repository.save(item);
    return saved.ok ? ok(saved.value) : fail([saved.error]);
  }

  find(predicate: (item: T) => boolean): T | undefined {
    return this.repository.list(predicate)[0];
  }
}

export class EventBus<Events extends Record<string, unknown>> {
  private readonly handlers: { [K in keyof Events]?: Array<(payload: Events[K]) => void> } = {};

  on<K extends keyof Events>(event: K, handler: (payload: Events[K]) => void): () => void {
    const list = this.handlers[event] ?? [];
    list.push(handler);
    this.handlers[event] = list;
    return () => {
      const current = this.handlers[event];
      if (current) {
        this.handlers[event] = current.filter((candidate) => candidate !== handler);
      }
    };
  }

  emit<K extends keyof Events>(event: K, payload: Events[K]): number {
    const list = this.handlers[event];
    if (!list) {
      return 0;
    }
    for (const handler of list) {
      handler(payload);
    }
    return list.length;
  }
}

export function groupBy<T, K extends PropertyKey>(items: readonly T[], key: (item: T) => K): Record<K, T[]> {
  const groups = {} as Record<K, T[]>;
  for (const item of items) {
    const group = key(item);
    (groups[group] ??= []).push(item);
  }
  return groups;
}

export function pick<T, K extends keyof T>(value: T, ...keys: K[]): Pick<T, K> {
  const result = {} as Pick<T, K>;
  for (const key of keys) {
    result[key] = value[key];
  }
  return result;
}

export function format(value: string): string;
export function format(value: number, digits?: number): string;
export function format(value: string | number, digits = 2): string {
  return typeof value === "number" ? value.toFixed(digits) : value.trim();
}

export async function retry<T>(attempts: number, run: () => Promise<T>): Promise<T> {
  let last: unknown;
  for (let attempt = 0; attempt < attempts; attempt++) {
    try {
      return await run();
    } catch (error) {
      last = error;
    }
  }
  throw last instanceof Error ? last : new Error(String(last));
}
