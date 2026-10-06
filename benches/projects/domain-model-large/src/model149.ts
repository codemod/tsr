// Generated domain module 149 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model148Service, summarizeModel148 } from "./model148";
import type { Model148 } from "./model148";
import { Model142Service, summarizeModel142 } from "./model142";
import type { Model142 } from "./model142";

export type Model149Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model149Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model149 extends Entity<"model149"> {
  updatedAt?: number;
  name: string;
  status: Model149Status;
  tags: string[];
  lines: Model149Line[];
  owner?: { name: string; email?: string };
  parent?: Model148;
  related: Model142[];
}

export type Model149Event =
  | { kind: "created"; item: Model149 }
  | { kind: "renamed"; id: Id<"model149">; from: string; to: string }
  | { kind: "moved"; id: Id<"model149">; status: Model149Status }
  | { kind: "deleted"; id: Id<"model149">; reason?: string };

export type Model149Events = {
  change: Model149Event;
  error: { message: string; code: number };
};

export type Model149Numbers = KeysOfType<Model149Line, number>;
export type FrozenModel149 = DeepReadonly<Model149>;

export function describeModel149Event(event: Model149Event): string {
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

export function totalModel149(item: Model149): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel149(item: Model149): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel149(item), 3), (item.parent ? summarizeModel148(item.parent) : "-"), item.related.map(summarizeModel142).join(",")].join(" | ");
}

export class Model149Service extends Service<Model149, "model149"> {
  readonly events = new EventBus<Model149Events>();
  private readonly parents?: Model148Service;

  constructor(repository = new MemoryRepository<Model149, "model149">()) {
    super(repository);
  }

  validate(item: Model149): string[] {
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

  rename(id: Id<"model149">, to: string): Result<Model149> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model149 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model149">, status: Model149Status): Result<Model149Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model149">, patch: Patch<Pick<Model149, "name" | "tags">>): Model149 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model149Status, Model149[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model149Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model149">[]): Promise<Model149[]> {
    const found: Model149[] = [];
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

  linkedService(): Model148Service {
    return this.parents ?? new Model148Service();
  }
}

export function makeModel149(id: string, name: string): Model149 {
  return {
    id: id as Id<"model149">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 150, unit: "hour" }],
    related: [],
  };
}

export const model149Defaults: FrozenModel149 = makeModel149("default-149", "Default 149");
export const model149Label = summarizeModel149(makeModel149("label", "Label"));
