// Generated domain module 54 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model053Service, summarizeModel053 } from "./model053";
import type { Model053 } from "./model053";
import { Model047Service, summarizeModel047 } from "./model047";
import type { Model047 } from "./model047";

export type Model054Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model054Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model054 extends Entity<"model054"> {
  updatedAt?: number;
  name: string;
  status: Model054Status;
  tags: string[];
  lines: Model054Line[];
  owner?: { name: string; email?: string };
  parent?: Model053;
  related: Model047[];
}

export type Model054Event =
  | { kind: "created"; item: Model054 }
  | { kind: "renamed"; id: Id<"model054">; from: string; to: string }
  | { kind: "moved"; id: Id<"model054">; status: Model054Status }
  | { kind: "deleted"; id: Id<"model054">; reason?: string };

export type Model054Events = {
  change: Model054Event;
  error: { message: string; code: number };
};

export type Model054Numbers = KeysOfType<Model054Line, number>;
export type FrozenModel054 = DeepReadonly<Model054>;

export function describeModel054Event(event: Model054Event): string {
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

export function totalModel054(item: Model054): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel054(item: Model054): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel054(item), 3), (item.parent ? summarizeModel053(item.parent) : "-"), item.related.map(summarizeModel047).join(",")].join(" | ");
}

export class Model054Service extends Service<Model054, "model054"> {
  readonly events = new EventBus<Model054Events>();
  private readonly parents?: Model053Service;

  constructor(repository = new MemoryRepository<Model054, "model054">()) {
    super(repository);
  }

  validate(item: Model054): string[] {
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

  rename(id: Id<"model054">, to: string): Result<Model054> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model054 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model054">, status: Model054Status): Result<Model054Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model054">, patch: Patch<Pick<Model054, "name" | "tags">>): Model054 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model054Status, Model054[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model054Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model054">[]): Promise<Model054[]> {
    const found: Model054[] = [];
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

  linkedService(): Model053Service {
    return this.parents ?? new Model053Service();
  }
}

export function makeModel054(id: string, name: string): Model054 {
  return {
    id: id as Id<"model054">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 55, unit: "hour" }],
    related: [],
  };
}

export const model054Defaults: FrozenModel054 = makeModel054("default-54", "Default 54");
export const model054Label = summarizeModel054(makeModel054("label", "Label"));
