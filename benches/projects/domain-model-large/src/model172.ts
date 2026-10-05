// Generated domain module 172 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model171Service, summarizeModel171 } from "./model171";
import type { Model171 } from "./model171";
import { Model165Service, summarizeModel165 } from "./model165";
import type { Model165 } from "./model165";

export type Model172Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model172Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model172 extends Entity<"model172"> {
  updatedAt?: number;
  name: string;
  status: Model172Status;
  tags: string[];
  lines: Model172Line[];
  owner?: { name: string; email?: string };
  parent?: Model171;
  related: Model165[];
}

export type Model172Event =
  | { kind: "created"; item: Model172 }
  | { kind: "renamed"; id: Id<"model172">; from: string; to: string }
  | { kind: "moved"; id: Id<"model172">; status: Model172Status }
  | { kind: "deleted"; id: Id<"model172">; reason?: string };

export type Model172Events = {
  change: Model172Event;
  error: { message: string; code: number };
};

export type Model172Numbers = KeysOfType<Model172Line, number>;
export type FrozenModel172 = DeepReadonly<Model172>;

export function describeModel172Event(event: Model172Event): string {
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

export function totalModel172(item: Model172): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel172(item: Model172): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel172(item), 3), (item.parent ? summarizeModel171(item.parent) : "-"), item.related.map(summarizeModel165).join(",")].join(" | ");
}

export class Model172Service extends Service<Model172, "model172"> {
  readonly events = new EventBus<Model172Events>();
  private readonly parents?: Model171Service;

  constructor(repository = new MemoryRepository<Model172, "model172">()) {
    super(repository);
  }

  validate(item: Model172): string[] {
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

  rename(id: Id<"model172">, to: string): Result<Model172> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model172 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model172">, status: Model172Status): Result<Model172Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model172">, patch: Patch<Pick<Model172, "name" | "tags">>): Model172 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model172Status, Model172[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model172Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model172">[]): Promise<Model172[]> {
    const found: Model172[] = [];
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

  linkedService(): Model171Service {
    return this.parents ?? new Model171Service();
  }
}

export function makeModel172(id: string, name: string): Model172 {
  return {
    id: id as Id<"model172">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 173, unit: "s" }],
    related: [],
  };
}

export const model172Defaults: FrozenModel172 = makeModel172("default-172", "Default 172");
export const model172Label = summarizeModel172(makeModel172("label", "Label"));
