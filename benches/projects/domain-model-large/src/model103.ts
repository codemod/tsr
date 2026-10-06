// Generated domain module 103 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model102Service, summarizeModel102 } from "./model102";
import type { Model102 } from "./model102";
import { Model096Service, summarizeModel096 } from "./model096";
import type { Model096 } from "./model096";

export type Model103Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model103Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model103 extends Entity<"model103"> {
  updatedAt?: number;
  name: string;
  status: Model103Status;
  tags: string[];
  lines: Model103Line[];
  owner?: { name: string; email?: string };
  parent?: Model102;
  related: Model096[];
}

export type Model103Event =
  | { kind: "created"; item: Model103 }
  | { kind: "renamed"; id: Id<"model103">; from: string; to: string }
  | { kind: "moved"; id: Id<"model103">; status: Model103Status }
  | { kind: "deleted"; id: Id<"model103">; reason?: string };

export type Model103Events = {
  change: Model103Event;
  error: { message: string; code: number };
};

export type Model103Numbers = KeysOfType<Model103Line, number>;
export type FrozenModel103 = DeepReadonly<Model103>;

export function describeModel103Event(event: Model103Event): string {
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

export function totalModel103(item: Model103): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel103(item: Model103): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel103(item), 3), (item.parent ? summarizeModel102(item.parent) : "-"), item.related.map(summarizeModel096).join(",")].join(" | ");
}

export class Model103Service extends Service<Model103, "model103"> {
  readonly events = new EventBus<Model103Events>();
  private readonly parents?: Model102Service;

  constructor(repository = new MemoryRepository<Model103, "model103">()) {
    super(repository);
  }

  validate(item: Model103): string[] {
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

  rename(id: Id<"model103">, to: string): Result<Model103> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model103 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model103">, status: Model103Status): Result<Model103Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model103">, patch: Patch<Pick<Model103, "name" | "tags">>): Model103 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model103Status, Model103[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model103Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model103">[]): Promise<Model103[]> {
    const found: Model103[] = [];
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

  linkedService(): Model102Service {
    return this.parents ?? new Model102Service();
  }
}

export function makeModel103(id: string, name: string): Model103 {
  return {
    id: id as Id<"model103">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 104, unit: "item" }],
    related: [],
  };
}

export const model103Defaults: FrozenModel103 = makeModel103("default-103", "Default 103");
export const model103Label = summarizeModel103(makeModel103("label", "Label"));
