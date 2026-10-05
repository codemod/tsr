// Generated domain module 187 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model186Service, summarizeModel186 } from "./model186";
import type { Model186 } from "./model186";
import { Model180Service, summarizeModel180 } from "./model180";
import type { Model180 } from "./model180";

export type Model187Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model187Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model187 extends Entity<"model187"> {
  updatedAt?: number;
  name: string;
  status: Model187Status;
  tags: string[];
  lines: Model187Line[];
  owner?: { name: string; email?: string };
  parent?: Model186;
  related: Model180[];
}

export type Model187Event =
  | { kind: "created"; item: Model187 }
  | { kind: "renamed"; id: Id<"model187">; from: string; to: string }
  | { kind: "moved"; id: Id<"model187">; status: Model187Status }
  | { kind: "deleted"; id: Id<"model187">; reason?: string };

export type Model187Events = {
  change: Model187Event;
  error: { message: string; code: number };
};

export type Model187Numbers = KeysOfType<Model187Line, number>;
export type FrozenModel187 = DeepReadonly<Model187>;

export function describeModel187Event(event: Model187Event): string {
  switch (event.kind) {
    case "created":
      return `created ${event.item.name}`;
    case "renamed":
      return `renamed ${event.from} -> ${event.to}`;
    case "moved":
      return `moved to ${event.status}`;
    case "deleted":
      return event.reason ? `deleted: ${event.reason}` : "deleted";
    default: {
      const unreachable: never = event;
      return unreachable;
    }
  }
}

export function totalModel187(item: Model187): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel187(item: Model187): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel187(item), 3), (item.parent ? summarizeModel186(item.parent) : "-"), item.related.map(summarizeModel180).join(",")].join(" | ");
}

export class Model187Service extends Service<Model187, "model187"> {
  readonly events = new EventBus<Model187Events>();
  private readonly parents?: Model186Service;

  constructor(repository = new MemoryRepository<Model187, "model187">()) {
    super(repository);
  }

  validate(item: Model187): string[] {
    const problems: string[] = [];
    if (item.name.trim().length === 0) {
      problems.push("name is required");
    }
    for (const [index, line] of item.lines.entries()) {
      if (line.quantity <= 0) {
        problems.push(`line ${index} has no quantity`);
      }
    }
    return problems;
  }

  rename(id: Id<"model187">, to: string): Result<Model187> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model187 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model187">, status: Model187Status): Result<Model187Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model187">, patch: Patch<Pick<Model187, "name" | "tags">>): Model187 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model187Status, Model187[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model187Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model187">[]): Promise<Model187[]> {
    const found: Model187[] = [];
    for (const id of ids) {
      const item = await retry(3, async () => this.find((candidate) => candidate.id === id));
      if (item) {
        found.push(item);
      }
    }
    return found;
  }

  parentNames(): string[] {
    return this.repository
      .list((item) => item.parent !== undefined)
      .map((item) => unwrapOr(ok(item.parent?.name ?? ""), ""));
  }

  linkedService(): Model186Service {
    return this.parents ?? new Model186Service();
  }
}

export function makeModel187(id: string, name: string): Model187 {
  return {
    id: id as Id<"model187">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 188, unit: "s" }],
    related: [],
  };
}

export const model187Defaults: FrozenModel187 = makeModel187("default-187", "Default 187");
export const model187Label = summarizeModel187(makeModel187("label", "Label"));
