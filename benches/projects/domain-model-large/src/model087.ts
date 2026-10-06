// Generated domain module 87 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model086Service, summarizeModel086 } from "./model086";
import type { Model086 } from "./model086";
import { Model080Service, summarizeModel080 } from "./model080";
import type { Model080 } from "./model080";

export type Model087Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model087Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model087 extends Entity<"model087"> {
  updatedAt?: number;
  name: string;
  status: Model087Status;
  tags: string[];
  lines: Model087Line[];
  owner?: { name: string; email?: string };
  parent?: Model086;
  related: Model080[];
}

export type Model087Event =
  | { kind: "created"; item: Model087 }
  | { kind: "renamed"; id: Id<"model087">; from: string; to: string }
  | { kind: "moved"; id: Id<"model087">; status: Model087Status }
  | { kind: "deleted"; id: Id<"model087">; reason?: string };

export type Model087Events = {
  change: Model087Event;
  error: { message: string; code: number };
};

export type Model087Numbers = KeysOfType<Model087Line, number>;
export type FrozenModel087 = DeepReadonly<Model087>;

export function describeModel087Event(event: Model087Event): string {
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

export function totalModel087(item: Model087): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel087(item: Model087): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel087(item), 3), (item.parent ? summarizeModel086(item.parent) : "-"), item.related.map(summarizeModel080).join(",")].join(" | ");
}

export class Model087Service extends Service<Model087, "model087"> {
  readonly events = new EventBus<Model087Events>();
  private readonly parents?: Model086Service;

  constructor(repository = new MemoryRepository<Model087, "model087">()) {
    super(repository);
  }

  validate(item: Model087): string[] {
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

  rename(id: Id<"model087">, to: string): Result<Model087> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model087 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model087">, status: Model087Status): Result<Model087Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model087">, patch: Patch<Pick<Model087, "name" | "tags">>): Model087 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model087Status, Model087[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model087Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model087">[]): Promise<Model087[]> {
    const found: Model087[] = [];
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

  linkedService(): Model086Service {
    return this.parents ?? new Model086Service();
  }
}

export function makeModel087(id: string, name: string): Model087 {
  return {
    id: id as Id<"model087">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 88, unit: "s" }],
    related: [],
  };
}

export const model087Defaults: FrozenModel087 = makeModel087("default-87", "Default 87");
export const model087Label = summarizeModel087(makeModel087("label", "Label"));
