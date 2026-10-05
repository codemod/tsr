// Generated domain module 112 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model111Service, summarizeModel111 } from "./model111";
import type { Model111 } from "./model111";
import { Model105Service, summarizeModel105 } from "./model105";
import type { Model105 } from "./model105";

export type Model112Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model112Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model112 extends Entity<"model112"> {
  updatedAt?: number;
  name: string;
  status: Model112Status;
  tags: string[];
  lines: Model112Line[];
  owner?: { name: string; email?: string };
  parent?: Model111;
  related: Model105[];
}

export type Model112Event =
  | { kind: "created"; item: Model112 }
  | { kind: "renamed"; id: Id<"model112">; from: string; to: string }
  | { kind: "moved"; id: Id<"model112">; status: Model112Status }
  | { kind: "deleted"; id: Id<"model112">; reason?: string };

export type Model112Events = {
  change: Model112Event;
  error: { message: string; code: number };
};

export type Model112Numbers = KeysOfType<Model112Line, number>;
export type FrozenModel112 = DeepReadonly<Model112>;

export function describeModel112Event(event: Model112Event): string {
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

export function totalModel112(item: Model112): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel112(item: Model112): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel112(item), 3), (item.parent ? summarizeModel111(item.parent) : "-"), item.related.map(summarizeModel105).join(",")].join(" | ");
}

export class Model112Service extends Service<Model112, "model112"> {
  readonly events = new EventBus<Model112Events>();
  private readonly parents?: Model111Service;

  constructor(repository = new MemoryRepository<Model112, "model112">()) {
    super(repository);
  }

  validate(item: Model112): string[] {
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

  rename(id: Id<"model112">, to: string): Result<Model112> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model112 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model112">, status: Model112Status): Result<Model112Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model112">, patch: Patch<Pick<Model112, "name" | "tags">>): Model112 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model112Status, Model112[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model112Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model112">[]): Promise<Model112[]> {
    const found: Model112[] = [];
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

  linkedService(): Model111Service {
    return this.parents ?? new Model111Service();
  }
}

export function makeModel112(id: string, name: string): Model112 {
  return {
    id: id as Id<"model112">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 113, unit: "s" }],
    related: [],
  };
}

export const model112Defaults: FrozenModel112 = makeModel112("default-112", "Default 112");
export const model112Label = summarizeModel112(makeModel112("label", "Label"));
