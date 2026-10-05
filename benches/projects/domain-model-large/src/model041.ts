// Generated domain module 41 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model040Service, summarizeModel040 } from "./model040";
import type { Model040 } from "./model040";
import { Model034Service, summarizeModel034 } from "./model034";
import type { Model034 } from "./model034";

export type Model041Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model041Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model041 extends Entity<"model041"> {
  updatedAt?: number;
  name: string;
  status: Model041Status;
  tags: string[];
  lines: Model041Line[];
  owner?: { name: string; email?: string };
  parent?: Model040;
  related: Model034[];
}

export type Model041Event =
  | { kind: "created"; item: Model041 }
  | { kind: "renamed"; id: Id<"model041">; from: string; to: string }
  | { kind: "moved"; id: Id<"model041">; status: Model041Status }
  | { kind: "deleted"; id: Id<"model041">; reason?: string };

export type Model041Events = {
  change: Model041Event;
  error: { message: string; code: number };
};

export type Model041Numbers = KeysOfType<Model041Line, number>;
export type FrozenModel041 = DeepReadonly<Model041>;

export function describeModel041Event(event: Model041Event): string {
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

export function totalModel041(item: Model041): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel041(item: Model041): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel041(item), 3), (item.parent ? summarizeModel040(item.parent) : "-"), item.related.map(summarizeModel034).join(",")].join(" | ");
}

export class Model041Service extends Service<Model041, "model041"> {
  readonly events = new EventBus<Model041Events>();
  private readonly parents?: Model040Service;

  constructor(repository = new MemoryRepository<Model041, "model041">()) {
    super(repository);
  }

  validate(item: Model041): string[] {
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

  rename(id: Id<"model041">, to: string): Result<Model041> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model041 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model041">, status: Model041Status): Result<Model041Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model041">, patch: Patch<Pick<Model041, "name" | "tags">>): Model041 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model041Status, Model041[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model041Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model041">[]): Promise<Model041[]> {
    const found: Model041[] = [];
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

  linkedService(): Model040Service {
    return this.parents ?? new Model040Service();
  }
}

export function makeModel041(id: string, name: string): Model041 {
  return {
    id: id as Id<"model041">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 42, unit: "m" }],
    related: [],
  };
}

export const model041Defaults: FrozenModel041 = makeModel041("default-41", "Default 41");
export const model041Label = summarizeModel041(makeModel041("label", "Label"));
