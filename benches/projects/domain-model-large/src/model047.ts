// Generated domain module 47 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model046Service, summarizeModel046 } from "./model046";
import type { Model046 } from "./model046";
import { Model040Service, summarizeModel040 } from "./model040";
import type { Model040 } from "./model040";

export type Model047Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model047Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model047 extends Entity<"model047"> {
  updatedAt?: number;
  name: string;
  status: Model047Status;
  tags: string[];
  lines: Model047Line[];
  owner?: { name: string; email?: string };
  parent?: Model046;
  related: Model040[];
}

export type Model047Event =
  | { kind: "created"; item: Model047 }
  | { kind: "renamed"; id: Id<"model047">; from: string; to: string }
  | { kind: "moved"; id: Id<"model047">; status: Model047Status }
  | { kind: "deleted"; id: Id<"model047">; reason?: string };

export type Model047Events = {
  change: Model047Event;
  error: { message: string; code: number };
};

export type Model047Numbers = KeysOfType<Model047Line, number>;
export type FrozenModel047 = DeepReadonly<Model047>;

export function describeModel047Event(event: Model047Event): string {
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

export function totalModel047(item: Model047): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel047(item: Model047): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel047(item), 3), (item.parent ? summarizeModel046(item.parent) : "-"), item.related.map(summarizeModel040).join(",")].join(" | ");
}

export class Model047Service extends Service<Model047, "model047"> {
  readonly events = new EventBus<Model047Events>();
  private readonly parents?: Model046Service;

  constructor(repository = new MemoryRepository<Model047, "model047">()) {
    super(repository);
  }

  validate(item: Model047): string[] {
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

  rename(id: Id<"model047">, to: string): Result<Model047> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model047 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model047">, status: Model047Status): Result<Model047Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model047">, patch: Patch<Pick<Model047, "name" | "tags">>): Model047 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model047Status, Model047[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model047Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model047">[]): Promise<Model047[]> {
    const found: Model047[] = [];
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

  linkedService(): Model046Service {
    return this.parents ?? new Model046Service();
  }
}

export function makeModel047(id: string, name: string): Model047 {
  return {
    id: id as Id<"model047">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 48, unit: "s" }],
    related: [],
  };
}

export const model047Defaults: FrozenModel047 = makeModel047("default-47", "Default 47");
export const model047Label = summarizeModel047(makeModel047("label", "Label"));
