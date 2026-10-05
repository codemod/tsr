// Generated domain module 5 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model004Service, summarizeModel004 } from "./model004";
import type { Model004 } from "./model004";

export type Model005Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model005Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model005 extends Entity<"model005"> {
  updatedAt?: number;
  name: string;
  status: Model005Status;
  tags: string[];
  lines: Model005Line[];
  owner?: { name: string; email?: string };
  parent?: Model004;
}

export type Model005Event =
  | { kind: "created"; item: Model005 }
  | { kind: "renamed"; id: Id<"model005">; from: string; to: string }
  | { kind: "moved"; id: Id<"model005">; status: Model005Status }
  | { kind: "deleted"; id: Id<"model005">; reason?: string };

export type Model005Events = {
  change: Model005Event;
  error: { message: string; code: number };
};

export type Model005Numbers = KeysOfType<Model005Line, number>;
export type FrozenModel005 = DeepReadonly<Model005>;

export function describeModel005Event(event: Model005Event): string {
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

export function totalModel005(item: Model005): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel005(item: Model005): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel005(item), 3), (item.parent ? summarizeModel004(item.parent) : "-")].join(" | ");
}

export class Model005Service extends Service<Model005, "model005"> {
  readonly events = new EventBus<Model005Events>();
  private readonly parents?: Model004Service;

  constructor(repository = new MemoryRepository<Model005, "model005">()) {
    super(repository);
  }

  validate(item: Model005): string[] {
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

  rename(id: Id<"model005">, to: string): Result<Model005> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model005 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model005">, status: Model005Status): Result<Model005Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model005">, patch: Patch<Pick<Model005, "name" | "tags">>): Model005 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model005Status, Model005[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model005Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model005">[]): Promise<Model005[]> {
    const found: Model005[] = [];
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

  linkedService(): Model004Service {
    return this.parents ?? new Model004Service();
  }
}

export function makeModel005(id: string, name: string): Model005 {
  return {
    id: id as Id<"model005">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 6, unit: "kg" }],
  };
}

export const model005Defaults: FrozenModel005 = makeModel005("default-5", "Default 5");
export const model005Label = summarizeModel005(makeModel005("label", "Label"));
