// Generated domain module 173 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model172Service, summarizeModel172 } from "./model172";
import type { Model172 } from "./model172";
import { Model166Service, summarizeModel166 } from "./model166";
import type { Model166 } from "./model166";

export type Model173Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model173Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model173 extends Entity<"model173"> {
  updatedAt?: number;
  name: string;
  status: Model173Status;
  tags: string[];
  lines: Model173Line[];
  owner?: { name: string; email?: string };
  parent?: Model172;
  related: Model166[];
}

export type Model173Event =
  | { kind: "created"; item: Model173 }
  | { kind: "renamed"; id: Id<"model173">; from: string; to: string }
  | { kind: "moved"; id: Id<"model173">; status: Model173Status }
  | { kind: "deleted"; id: Id<"model173">; reason?: string };

export type Model173Events = {
  change: Model173Event;
  error: { message: string; code: number };
};

export type Model173Numbers = KeysOfType<Model173Line, number>;
export type FrozenModel173 = DeepReadonly<Model173>;

export function describeModel173Event(event: Model173Event): string {
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

export function totalModel173(item: Model173): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel173(item: Model173): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel173(item), 3), (item.parent ? summarizeModel172(item.parent) : "-"), item.related.map(summarizeModel166).join(",")].join(" | ");
}

export class Model173Service extends Service<Model173, "model173"> {
  readonly events = new EventBus<Model173Events>();
  private readonly parents?: Model172Service;

  constructor(repository = new MemoryRepository<Model173, "model173">()) {
    super(repository);
  }

  validate(item: Model173): string[] {
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

  rename(id: Id<"model173">, to: string): Result<Model173> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model173 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model173">, status: Model173Status): Result<Model173Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model173">, patch: Patch<Pick<Model173, "name" | "tags">>): Model173 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model173Status, Model173[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model173Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model173">[]): Promise<Model173[]> {
    const found: Model173[] = [];
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

  linkedService(): Model172Service {
    return this.parents ?? new Model172Service();
  }
}

export function makeModel173(id: string, name: string): Model173 {
  return {
    id: id as Id<"model173">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 174, unit: "item" }],
    related: [],
  };
}

export const model173Defaults: FrozenModel173 = makeModel173("default-173", "Default 173");
export const model173Label = summarizeModel173(makeModel173("label", "Label"));
