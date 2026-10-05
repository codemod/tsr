// Generated domain module 91 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model090Service, summarizeModel090 } from "./model090";
import type { Model090 } from "./model090";
import { Model084Service, summarizeModel084 } from "./model084";
import type { Model084 } from "./model084";

export type Model091Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model091Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model091 extends Entity<"model091"> {
  updatedAt?: number;
  name: string;
  status: Model091Status;
  tags: string[];
  lines: Model091Line[];
  owner?: { name: string; email?: string };
  parent?: Model090;
  related: Model084[];
}

export type Model091Event =
  | { kind: "created"; item: Model091 }
  | { kind: "renamed"; id: Id<"model091">; from: string; to: string }
  | { kind: "moved"; id: Id<"model091">; status: Model091Status }
  | { kind: "deleted"; id: Id<"model091">; reason?: string };

export type Model091Events = {
  change: Model091Event;
  error: { message: string; code: number };
};

export type Model091Numbers = KeysOfType<Model091Line, number>;
export type FrozenModel091 = DeepReadonly<Model091>;

export function describeModel091Event(event: Model091Event): string {
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

export function totalModel091(item: Model091): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel091(item: Model091): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel091(item), 3), (item.parent ? summarizeModel090(item.parent) : "-"), item.related.map(summarizeModel084).join(",")].join(" | ");
}

export class Model091Service extends Service<Model091, "model091"> {
  readonly events = new EventBus<Model091Events>();
  private readonly parents?: Model090Service;

  constructor(repository = new MemoryRepository<Model091, "model091">()) {
    super(repository);
  }

  validate(item: Model091): string[] {
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

  rename(id: Id<"model091">, to: string): Result<Model091> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model091 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model091">, status: Model091Status): Result<Model091Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model091">, patch: Patch<Pick<Model091, "name" | "tags">>): Model091 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model091Status, Model091[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model091Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model091">[]): Promise<Model091[]> {
    const found: Model091[] = [];
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

  linkedService(): Model090Service {
    return this.parents ?? new Model090Service();
  }
}

export function makeModel091(id: string, name: string): Model091 {
  return {
    id: id as Id<"model091">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 92, unit: "m" }],
    related: [],
  };
}

export const model091Defaults: FrozenModel091 = makeModel091("default-91", "Default 91");
export const model091Label = summarizeModel091(makeModel091("label", "Label"));
