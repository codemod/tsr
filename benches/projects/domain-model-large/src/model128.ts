// Generated domain module 128 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model127Service, summarizeModel127 } from "./model127";
import type { Model127 } from "./model127";
import { Model121Service, summarizeModel121 } from "./model121";
import type { Model121 } from "./model121";

export type Model128Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model128Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model128 extends Entity<"model128"> {
  updatedAt?: number;
  name: string;
  status: Model128Status;
  tags: string[];
  lines: Model128Line[];
  owner?: { name: string; email?: string };
  parent?: Model127;
  related: Model121[];
}

export type Model128Event =
  | { kind: "created"; item: Model128 }
  | { kind: "renamed"; id: Id<"model128">; from: string; to: string }
  | { kind: "moved"; id: Id<"model128">; status: Model128Status }
  | { kind: "deleted"; id: Id<"model128">; reason?: string };

export type Model128Events = {
  change: Model128Event;
  error: { message: string; code: number };
};

export type Model128Numbers = KeysOfType<Model128Line, number>;
export type FrozenModel128 = DeepReadonly<Model128>;

export function describeModel128Event(event: Model128Event): string {
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

export function totalModel128(item: Model128): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel128(item: Model128): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel128(item), 3), (item.parent ? summarizeModel127(item.parent) : "-"), item.related.map(summarizeModel121).join(",")].join(" | ");
}

export class Model128Service extends Service<Model128, "model128"> {
  readonly events = new EventBus<Model128Events>();
  private readonly parents?: Model127Service;

  constructor(repository = new MemoryRepository<Model128, "model128">()) {
    super(repository);
  }

  validate(item: Model128): string[] {
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

  rename(id: Id<"model128">, to: string): Result<Model128> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model128 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model128">, status: Model128Status): Result<Model128Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model128">, patch: Patch<Pick<Model128, "name" | "tags">>): Model128 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model128Status, Model128[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model128Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model128">[]): Promise<Model128[]> {
    const found: Model128[] = [];
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

  linkedService(): Model127Service {
    return this.parents ?? new Model127Service();
  }
}

export function makeModel128(id: string, name: string): Model128 {
  return {
    id: id as Id<"model128">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 129, unit: "item" }],
    related: [],
  };
}

export const model128Defaults: FrozenModel128 = makeModel128("default-128", "Default 128");
export const model128Label = summarizeModel128(makeModel128("label", "Label"));
