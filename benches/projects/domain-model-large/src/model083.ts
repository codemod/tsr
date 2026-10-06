// Generated domain module 83 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model082Service, summarizeModel082 } from "./model082";
import type { Model082 } from "./model082";
import { Model076Service, summarizeModel076 } from "./model076";
import type { Model076 } from "./model076";

export type Model083Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model083Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model083 extends Entity<"model083"> {
  updatedAt?: number;
  name: string;
  status: Model083Status;
  tags: string[];
  lines: Model083Line[];
  owner?: { name: string; email?: string };
  parent?: Model082;
  related: Model076[];
}

export type Model083Event =
  | { kind: "created"; item: Model083 }
  | { kind: "renamed"; id: Id<"model083">; from: string; to: string }
  | { kind: "moved"; id: Id<"model083">; status: Model083Status }
  | { kind: "deleted"; id: Id<"model083">; reason?: string };

export type Model083Events = {
  change: Model083Event;
  error: { message: string; code: number };
};

export type Model083Numbers = KeysOfType<Model083Line, number>;
export type FrozenModel083 = DeepReadonly<Model083>;

export function describeModel083Event(event: Model083Event): string {
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

export function totalModel083(item: Model083): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel083(item: Model083): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel083(item), 3), (item.parent ? summarizeModel082(item.parent) : "-"), item.related.map(summarizeModel076).join(",")].join(" | ");
}

export class Model083Service extends Service<Model083, "model083"> {
  readonly events = new EventBus<Model083Events>();
  private readonly parents?: Model082Service;

  constructor(repository = new MemoryRepository<Model083, "model083">()) {
    super(repository);
  }

  validate(item: Model083): string[] {
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

  rename(id: Id<"model083">, to: string): Result<Model083> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model083 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model083">, status: Model083Status): Result<Model083Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model083">, patch: Patch<Pick<Model083, "name" | "tags">>): Model083 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model083Status, Model083[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model083Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model083">[]): Promise<Model083[]> {
    const found: Model083[] = [];
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

  linkedService(): Model082Service {
    return this.parents ?? new Model082Service();
  }
}

export function makeModel083(id: string, name: string): Model083 {
  return {
    id: id as Id<"model083">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 84, unit: "item" }],
    related: [],
  };
}

export const model083Defaults: FrozenModel083 = makeModel083("default-83", "Default 83");
export const model083Label = summarizeModel083(makeModel083("label", "Label"));
