// Generated domain module 98 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model097Service, summarizeModel097 } from "./model097";
import type { Model097 } from "./model097";
import { Model091Service, summarizeModel091 } from "./model091";
import type { Model091 } from "./model091";

export type Model098Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model098Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model098 extends Entity<"model098"> {
  updatedAt?: number;
  name: string;
  status: Model098Status;
  tags: string[];
  lines: Model098Line[];
  owner?: { name: string; email?: string };
  parent?: Model097;
  related: Model091[];
}

export type Model098Event =
  | { kind: "created"; item: Model098 }
  | { kind: "renamed"; id: Id<"model098">; from: string; to: string }
  | { kind: "moved"; id: Id<"model098">; status: Model098Status }
  | { kind: "deleted"; id: Id<"model098">; reason?: string };

export type Model098Events = {
  change: Model098Event;
  error: { message: string; code: number };
};

export type Model098Numbers = KeysOfType<Model098Line, number>;
export type FrozenModel098 = DeepReadonly<Model098>;

export function describeModel098Event(event: Model098Event): string {
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

export function totalModel098(item: Model098): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel098(item: Model098): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel098(item), 3), (item.parent ? summarizeModel097(item.parent) : "-"), item.related.map(summarizeModel091).join(",")].join(" | ");
}

export class Model098Service extends Service<Model098, "model098"> {
  readonly events = new EventBus<Model098Events>();
  private readonly parents?: Model097Service;

  constructor(repository = new MemoryRepository<Model098, "model098">()) {
    super(repository);
  }

  validate(item: Model098): string[] {
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

  rename(id: Id<"model098">, to: string): Result<Model098> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model098 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model098">, status: Model098Status): Result<Model098Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model098">, patch: Patch<Pick<Model098, "name" | "tags">>): Model098 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model098Status, Model098[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model098Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model098">[]): Promise<Model098[]> {
    const found: Model098[] = [];
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

  linkedService(): Model097Service {
    return this.parents ?? new Model097Service();
  }
}

export function makeModel098(id: string, name: string): Model098 {
  return {
    id: id as Id<"model098">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 99, unit: "item" }],
    related: [],
  };
}

export const model098Defaults: FrozenModel098 = makeModel098("default-98", "Default 98");
export const model098Label = summarizeModel098(makeModel098("label", "Label"));
