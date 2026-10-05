// Generated domain module 13 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model012Service, summarizeModel012 } from "./model012";
import type { Model012 } from "./model012";
import { Model006Service, summarizeModel006 } from "./model006";
import type { Model006 } from "./model006";

export type Model013Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model013Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model013 extends Entity<"model013"> {
  updatedAt?: number;
  name: string;
  status: Model013Status;
  tags: string[];
  lines: Model013Line[];
  owner?: { name: string; email?: string };
  parent?: Model012;
  related: Model006[];
}

export type Model013Event =
  | { kind: "created"; item: Model013 }
  | { kind: "renamed"; id: Id<"model013">; from: string; to: string }
  | { kind: "moved"; id: Id<"model013">; status: Model013Status }
  | { kind: "deleted"; id: Id<"model013">; reason?: string };

export type Model013Events = {
  change: Model013Event;
  error: { message: string; code: number };
};

export type Model013Numbers = KeysOfType<Model013Line, number>;
export type FrozenModel013 = DeepReadonly<Model013>;

export function describeModel013Event(event: Model013Event): string {
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

export function totalModel013(item: Model013): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel013(item: Model013): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel013(item), 3), (item.parent ? summarizeModel012(item.parent) : "-"), item.related.map(summarizeModel006).join(",")].join(" | ");
}

export class Model013Service extends Service<Model013, "model013"> {
  readonly events = new EventBus<Model013Events>();
  private readonly parents?: Model012Service;

  constructor(repository = new MemoryRepository<Model013, "model013">()) {
    super(repository);
  }

  validate(item: Model013): string[] {
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

  rename(id: Id<"model013">, to: string): Result<Model013> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model013 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model013">, status: Model013Status): Result<Model013Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model013">, patch: Patch<Pick<Model013, "name" | "tags">>): Model013 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model013Status, Model013[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model013Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model013">[]): Promise<Model013[]> {
    const found: Model013[] = [];
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

  linkedService(): Model012Service {
    return this.parents ?? new Model012Service();
  }
}

export function makeModel013(id: string, name: string): Model013 {
  return {
    id: id as Id<"model013">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 14, unit: "item" }],
    related: [],
  };
}

export const model013Defaults: FrozenModel013 = makeModel013("default-13", "Default 13");
export const model013Label = summarizeModel013(makeModel013("label", "Label"));
