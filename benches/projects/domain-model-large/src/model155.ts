// Generated domain module 155 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model154Service, summarizeModel154 } from "./model154";
import type { Model154 } from "./model154";
import { Model148Service, summarizeModel148 } from "./model148";
import type { Model148 } from "./model148";

export type Model155Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model155Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model155 extends Entity<"model155"> {
  updatedAt?: number;
  name: string;
  status: Model155Status;
  tags: string[];
  lines: Model155Line[];
  owner?: { name: string; email?: string };
  parent?: Model154;
  related: Model148[];
}

export type Model155Event =
  | { kind: "created"; item: Model155 }
  | { kind: "renamed"; id: Id<"model155">; from: string; to: string }
  | { kind: "moved"; id: Id<"model155">; status: Model155Status }
  | { kind: "deleted"; id: Id<"model155">; reason?: string };

export type Model155Events = {
  change: Model155Event;
  error: { message: string; code: number };
};

export type Model155Numbers = KeysOfType<Model155Line, number>;
export type FrozenModel155 = DeepReadonly<Model155>;

export function describeModel155Event(event: Model155Event): string {
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

export function totalModel155(item: Model155): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel155(item: Model155): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel155(item), 3), (item.parent ? summarizeModel154(item.parent) : "-"), item.related.map(summarizeModel148).join(",")].join(" | ");
}

export class Model155Service extends Service<Model155, "model155"> {
  readonly events = new EventBus<Model155Events>();
  private readonly parents?: Model154Service;

  constructor(repository = new MemoryRepository<Model155, "model155">()) {
    super(repository);
  }

  validate(item: Model155): string[] {
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

  rename(id: Id<"model155">, to: string): Result<Model155> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model155 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model155">, status: Model155Status): Result<Model155Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model155">, patch: Patch<Pick<Model155, "name" | "tags">>): Model155 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model155Status, Model155[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model155Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model155">[]): Promise<Model155[]> {
    const found: Model155[] = [];
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

  linkedService(): Model154Service {
    return this.parents ?? new Model154Service();
  }
}

export function makeModel155(id: string, name: string): Model155 {
  return {
    id: id as Id<"model155">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 156, unit: "kg" }],
    related: [],
  };
}

export const model155Defaults: FrozenModel155 = makeModel155("default-155", "Default 155");
export const model155Label = summarizeModel155(makeModel155("label", "Label"));
