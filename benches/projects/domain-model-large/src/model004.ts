// Generated domain module 4 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model003Service, summarizeModel003 } from "./model003";
import type { Model003 } from "./model003";

export type Model004Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model004Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model004 extends Entity<"model004"> {
  updatedAt?: number;
  name: string;
  status: Model004Status;
  tags: string[];
  lines: Model004Line[];
  owner?: { name: string; email?: string };
  parent?: Model003;
}

export type Model004Event =
  | { kind: "created"; item: Model004 }
  | { kind: "renamed"; id: Id<"model004">; from: string; to: string }
  | { kind: "moved"; id: Id<"model004">; status: Model004Status }
  | { kind: "deleted"; id: Id<"model004">; reason?: string };

export type Model004Events = {
  change: Model004Event;
  error: { message: string; code: number };
};

export type Model004Numbers = KeysOfType<Model004Line, number>;
export type FrozenModel004 = DeepReadonly<Model004>;

export function describeModel004Event(event: Model004Event): string {
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

export function totalModel004(item: Model004): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel004(item: Model004): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel004(item), 3), (item.parent ? summarizeModel003(item.parent) : "-")].join(" | ");
}

export class Model004Service extends Service<Model004, "model004"> {
  readonly events = new EventBus<Model004Events>();
  private readonly parents?: Model003Service;

  constructor(repository = new MemoryRepository<Model004, "model004">()) {
    super(repository);
  }

  validate(item: Model004): string[] {
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

  rename(id: Id<"model004">, to: string): Result<Model004> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model004 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model004">, status: Model004Status): Result<Model004Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model004">, patch: Patch<Pick<Model004, "name" | "tags">>): Model004 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model004Status, Model004[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model004Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model004">[]): Promise<Model004[]> {
    const found: Model004[] = [];
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

  linkedService(): Model003Service {
    return this.parents ?? new Model003Service();
  }
}

export function makeModel004(id: string, name: string): Model004 {
  return {
    id: id as Id<"model004">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 5, unit: "hour" }],
  };
}

export const model004Defaults: FrozenModel004 = makeModel004("default-4", "Default 4");
export const model004Label = summarizeModel004(makeModel004("label", "Label"));
