// Generated domain module 50 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model049Service, summarizeModel049 } from "./model049";
import type { Model049 } from "./model049";
import { Model043Service, summarizeModel043 } from "./model043";
import type { Model043 } from "./model043";

export type Model050Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model050Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model050 extends Entity<"model050"> {
  updatedAt?: number;
  name: string;
  status: Model050Status;
  tags: string[];
  lines: Model050Line[];
  owner?: { name: string; email?: string };
  parent?: Model049;
  related: Model043[];
}

export type Model050Event =
  | { kind: "created"; item: Model050 }
  | { kind: "renamed"; id: Id<"model050">; from: string; to: string }
  | { kind: "moved"; id: Id<"model050">; status: Model050Status }
  | { kind: "deleted"; id: Id<"model050">; reason?: string };

export type Model050Events = {
  change: Model050Event;
  error: { message: string; code: number };
};

export type Model050Numbers = KeysOfType<Model050Line, number>;
export type FrozenModel050 = DeepReadonly<Model050>;

export function describeModel050Event(event: Model050Event): string {
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

export function totalModel050(item: Model050): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel050(item: Model050): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel050(item), 3), (item.parent ? summarizeModel049(item.parent) : "-"), item.related.map(summarizeModel043).join(",")].join(" | ");
}

export class Model050Service extends Service<Model050, "model050"> {
  readonly events = new EventBus<Model050Events>();
  private readonly parents?: Model049Service;

  constructor(repository = new MemoryRepository<Model050, "model050">()) {
    super(repository);
  }

  validate(item: Model050): string[] {
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

  rename(id: Id<"model050">, to: string): Result<Model050> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model050 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model050">, status: Model050Status): Result<Model050Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model050">, patch: Patch<Pick<Model050, "name" | "tags">>): Model050 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model050Status, Model050[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model050Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model050">[]): Promise<Model050[]> {
    const found: Model050[] = [];
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

  linkedService(): Model049Service {
    return this.parents ?? new Model049Service();
  }
}

export function makeModel050(id: string, name: string): Model050 {
  return {
    id: id as Id<"model050">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 51, unit: "kg" }],
    related: [],
  };
}

export const model050Defaults: FrozenModel050 = makeModel050("default-50", "Default 50");
export const model050Label = summarizeModel050(makeModel050("label", "Label"));
