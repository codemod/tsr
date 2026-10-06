// Generated domain module 162 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model161Service, summarizeModel161 } from "./model161";
import type { Model161 } from "./model161";
import { Model155Service, summarizeModel155 } from "./model155";
import type { Model155 } from "./model155";

export type Model162Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model162Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model162 extends Entity<"model162"> {
  updatedAt?: number;
  name: string;
  status: Model162Status;
  tags: string[];
  lines: Model162Line[];
  owner?: { name: string; email?: string };
  parent?: Model161;
  related: Model155[];
}

export type Model162Event =
  | { kind: "created"; item: Model162 }
  | { kind: "renamed"; id: Id<"model162">; from: string; to: string }
  | { kind: "moved"; id: Id<"model162">; status: Model162Status }
  | { kind: "deleted"; id: Id<"model162">; reason?: string };

export type Model162Events = {
  change: Model162Event;
  error: { message: string; code: number };
};

export type Model162Numbers = KeysOfType<Model162Line, number>;
export type FrozenModel162 = DeepReadonly<Model162>;

export function describeModel162Event(event: Model162Event): string {
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

export function totalModel162(item: Model162): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel162(item: Model162): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel162(item), 3), (item.parent ? summarizeModel161(item.parent) : "-"), item.related.map(summarizeModel155).join(",")].join(" | ");
}

export class Model162Service extends Service<Model162, "model162"> {
  readonly events = new EventBus<Model162Events>();
  private readonly parents?: Model161Service;

  constructor(repository = new MemoryRepository<Model162, "model162">()) {
    super(repository);
  }

  validate(item: Model162): string[] {
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

  rename(id: Id<"model162">, to: string): Result<Model162> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model162 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model162">, status: Model162Status): Result<Model162Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model162">, patch: Patch<Pick<Model162, "name" | "tags">>): Model162 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model162Status, Model162[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model162Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model162">[]): Promise<Model162[]> {
    const found: Model162[] = [];
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

  linkedService(): Model161Service {
    return this.parents ?? new Model161Service();
  }
}

export function makeModel162(id: string, name: string): Model162 {
  return {
    id: id as Id<"model162">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 163, unit: "s" }],
    related: [],
  };
}

export const model162Defaults: FrozenModel162 = makeModel162("default-162", "Default 162");
export const model162Label = summarizeModel162(makeModel162("label", "Label"));
