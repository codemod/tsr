// Generated domain module 12 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model011Service, summarizeModel011 } from "./model011";
import type { Model011 } from "./model011";
import { Model005Service, summarizeModel005 } from "./model005";
import type { Model005 } from "./model005";

export type Model012Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model012Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model012 extends Entity<"model012"> {
  updatedAt?: number;
  name: string;
  status: Model012Status;
  tags: string[];
  lines: Model012Line[];
  owner?: { name: string; email?: string };
  parent?: Model011;
  related: Model005[];
}

export type Model012Event =
  | { kind: "created"; item: Model012 }
  | { kind: "renamed"; id: Id<"model012">; from: string; to: string }
  | { kind: "moved"; id: Id<"model012">; status: Model012Status }
  | { kind: "deleted"; id: Id<"model012">; reason?: string };

export type Model012Events = {
  change: Model012Event;
  error: { message: string; code: number };
};

export type Model012Numbers = KeysOfType<Model012Line, number>;
export type FrozenModel012 = DeepReadonly<Model012>;

export function describeModel012Event(event: Model012Event): string {
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

export function totalModel012(item: Model012): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel012(item: Model012): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel012(item), 3), (item.parent ? summarizeModel011(item.parent) : "-"), item.related.map(summarizeModel005).join(",")].join(" | ");
}

export class Model012Service extends Service<Model012, "model012"> {
  readonly events = new EventBus<Model012Events>();
  private readonly parents?: Model011Service;

  constructor(repository = new MemoryRepository<Model012, "model012">()) {
    super(repository);
  }

  validate(item: Model012): string[] {
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

  rename(id: Id<"model012">, to: string): Result<Model012> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model012 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model012">, status: Model012Status): Result<Model012Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model012">, patch: Patch<Pick<Model012, "name" | "tags">>): Model012 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model012Status, Model012[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model012Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model012">[]): Promise<Model012[]> {
    const found: Model012[] = [];
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

  linkedService(): Model011Service {
    return this.parents ?? new Model011Service();
  }
}

export function makeModel012(id: string, name: string): Model012 {
  return {
    id: id as Id<"model012">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 13, unit: "s" }],
    related: [],
  };
}

export const model012Defaults: FrozenModel012 = makeModel012("default-12", "Default 12");
export const model012Label = summarizeModel012(makeModel012("label", "Label"));
