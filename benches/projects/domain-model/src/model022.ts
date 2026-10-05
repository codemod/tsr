// Generated domain module 22 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model021Service, summarizeModel021 } from "./model021";
import type { Model021 } from "./model021";
import { Model015Service, summarizeModel015 } from "./model015";
import type { Model015 } from "./model015";

export type Model022Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model022Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model022 extends Entity<"model022"> {
  updatedAt?: number;
  name: string;
  status: Model022Status;
  tags: string[];
  lines: Model022Line[];
  owner?: { name: string; email?: string };
  parent?: Model021;
  related: Model015[];
}

export type Model022Event =
  | { kind: "created"; item: Model022 }
  | { kind: "renamed"; id: Id<"model022">; from: string; to: string }
  | { kind: "moved"; id: Id<"model022">; status: Model022Status }
  | { kind: "deleted"; id: Id<"model022">; reason?: string };

export type Model022Events = {
  change: Model022Event;
  error: { message: string; code: number };
};

export type Model022Numbers = KeysOfType<Model022Line, number>;
export type FrozenModel022 = DeepReadonly<Model022>;

export function describeModel022Event(event: Model022Event): string {
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

export function totalModel022(item: Model022): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel022(item: Model022): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel022(item), 3), (item.parent ? summarizeModel021(item.parent) : "-"), item.related.map(summarizeModel015).join(",")].join(" | ");
}

export class Model022Service extends Service<Model022, "model022"> {
  readonly events = new EventBus<Model022Events>();
  private readonly parents?: Model021Service;

  constructor(repository = new MemoryRepository<Model022, "model022">()) {
    super(repository);
  }

  validate(item: Model022): string[] {
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

  rename(id: Id<"model022">, to: string): Result<Model022> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model022 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model022">, status: Model022Status): Result<Model022Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model022">, patch: Patch<Pick<Model022, "name" | "tags">>): Model022 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model022Status, Model022[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model022Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model022">[]): Promise<Model022[]> {
    const found: Model022[] = [];
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

  linkedService(): Model021Service {
    return this.parents ?? new Model021Service();
  }
}

export function makeModel022(id: string, name: string): Model022 {
  return {
    id: id as Id<"model022">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 23, unit: "s" }],
    related: [],
  };
}

export const model022Defaults: FrozenModel022 = makeModel022("default-22", "Default 22");
export const model022Label = summarizeModel022(makeModel022("label", "Label"));
