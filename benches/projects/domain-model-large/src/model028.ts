// Generated domain module 28 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model027Service, summarizeModel027 } from "./model027";
import type { Model027 } from "./model027";
import { Model021Service, summarizeModel021 } from "./model021";
import type { Model021 } from "./model021";

export type Model028Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model028Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model028 extends Entity<"model028"> {
  updatedAt?: number;
  name: string;
  status: Model028Status;
  tags: string[];
  lines: Model028Line[];
  owner?: { name: string; email?: string };
  parent?: Model027;
  related: Model021[];
}

export type Model028Event =
  | { kind: "created"; item: Model028 }
  | { kind: "renamed"; id: Id<"model028">; from: string; to: string }
  | { kind: "moved"; id: Id<"model028">; status: Model028Status }
  | { kind: "deleted"; id: Id<"model028">; reason?: string };

export type Model028Events = {
  change: Model028Event;
  error: { message: string; code: number };
};

export type Model028Numbers = KeysOfType<Model028Line, number>;
export type FrozenModel028 = DeepReadonly<Model028>;

export function describeModel028Event(event: Model028Event): string {
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

export function totalModel028(item: Model028): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel028(item: Model028): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel028(item), 3), (item.parent ? summarizeModel027(item.parent) : "-"), item.related.map(summarizeModel021).join(",")].join(" | ");
}

export class Model028Service extends Service<Model028, "model028"> {
  readonly events = new EventBus<Model028Events>();
  private readonly parents?: Model027Service;

  constructor(repository = new MemoryRepository<Model028, "model028">()) {
    super(repository);
  }

  validate(item: Model028): string[] {
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

  rename(id: Id<"model028">, to: string): Result<Model028> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model028 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model028">, status: Model028Status): Result<Model028Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model028">, patch: Patch<Pick<Model028, "name" | "tags">>): Model028 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model028Status, Model028[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model028Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model028">[]): Promise<Model028[]> {
    const found: Model028[] = [];
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

  linkedService(): Model027Service {
    return this.parents ?? new Model027Service();
  }
}

export function makeModel028(id: string, name: string): Model028 {
  return {
    id: id as Id<"model028">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 29, unit: "item" }],
    related: [],
  };
}

export const model028Defaults: FrozenModel028 = makeModel028("default-28", "Default 28");
export const model028Label = summarizeModel028(makeModel028("label", "Label"));
