// Generated domain module 159 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model158Service, summarizeModel158 } from "./model158";
import type { Model158 } from "./model158";
import { Model152Service, summarizeModel152 } from "./model152";
import type { Model152 } from "./model152";

export type Model159Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model159Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model159 extends Entity<"model159"> {
  updatedAt?: number;
  name: string;
  status: Model159Status;
  tags: string[];
  lines: Model159Line[];
  owner?: { name: string; email?: string };
  parent?: Model158;
  related: Model152[];
}

export type Model159Event =
  | { kind: "created"; item: Model159 }
  | { kind: "renamed"; id: Id<"model159">; from: string; to: string }
  | { kind: "moved"; id: Id<"model159">; status: Model159Status }
  | { kind: "deleted"; id: Id<"model159">; reason?: string };

export type Model159Events = {
  change: Model159Event;
  error: { message: string; code: number };
};

export type Model159Numbers = KeysOfType<Model159Line, number>;
export type FrozenModel159 = DeepReadonly<Model159>;

export function describeModel159Event(event: Model159Event): string {
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

export function totalModel159(item: Model159): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel159(item: Model159): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel159(item), 3), (item.parent ? summarizeModel158(item.parent) : "-"), item.related.map(summarizeModel152).join(",")].join(" | ");
}

export class Model159Service extends Service<Model159, "model159"> {
  readonly events = new EventBus<Model159Events>();
  private readonly parents?: Model158Service;

  constructor(repository = new MemoryRepository<Model159, "model159">()) {
    super(repository);
  }

  validate(item: Model159): string[] {
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

  rename(id: Id<"model159">, to: string): Result<Model159> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model159 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model159">, status: Model159Status): Result<Model159Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model159">, patch: Patch<Pick<Model159, "name" | "tags">>): Model159 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model159Status, Model159[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model159Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model159">[]): Promise<Model159[]> {
    const found: Model159[] = [];
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

  linkedService(): Model158Service {
    return this.parents ?? new Model158Service();
  }
}

export function makeModel159(id: string, name: string): Model159 {
  return {
    id: id as Id<"model159">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 160, unit: "hour" }],
    related: [],
  };
}

export const model159Defaults: FrozenModel159 = makeModel159("default-159", "Default 159");
export const model159Label = summarizeModel159(makeModel159("label", "Label"));
