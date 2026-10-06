// Generated domain module 134 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model133Service, summarizeModel133 } from "./model133";
import type { Model133 } from "./model133";
import { Model127Service, summarizeModel127 } from "./model127";
import type { Model127 } from "./model127";

export type Model134Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model134Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model134 extends Entity<"model134"> {
  updatedAt?: number;
  name: string;
  status: Model134Status;
  tags: string[];
  lines: Model134Line[];
  owner?: { name: string; email?: string };
  parent?: Model133;
  related: Model127[];
}

export type Model134Event =
  | { kind: "created"; item: Model134 }
  | { kind: "renamed"; id: Id<"model134">; from: string; to: string }
  | { kind: "moved"; id: Id<"model134">; status: Model134Status }
  | { kind: "deleted"; id: Id<"model134">; reason?: string };

export type Model134Events = {
  change: Model134Event;
  error: { message: string; code: number };
};

export type Model134Numbers = KeysOfType<Model134Line, number>;
export type FrozenModel134 = DeepReadonly<Model134>;

export function describeModel134Event(event: Model134Event): string {
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

export function totalModel134(item: Model134): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel134(item: Model134): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel134(item), 3), (item.parent ? summarizeModel133(item.parent) : "-"), item.related.map(summarizeModel127).join(",")].join(" | ");
}

export class Model134Service extends Service<Model134, "model134"> {
  readonly events = new EventBus<Model134Events>();
  private readonly parents?: Model133Service;

  constructor(repository = new MemoryRepository<Model134, "model134">()) {
    super(repository);
  }

  validate(item: Model134): string[] {
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

  rename(id: Id<"model134">, to: string): Result<Model134> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model134 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model134">, status: Model134Status): Result<Model134Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model134">, patch: Patch<Pick<Model134, "name" | "tags">>): Model134 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model134Status, Model134[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model134Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model134">[]): Promise<Model134[]> {
    const found: Model134[] = [];
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

  linkedService(): Model133Service {
    return this.parents ?? new Model133Service();
  }
}

export function makeModel134(id: string, name: string): Model134 {
  return {
    id: id as Id<"model134">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 135, unit: "hour" }],
    related: [],
  };
}

export const model134Defaults: FrozenModel134 = makeModel134("default-134", "Default 134");
export const model134Label = summarizeModel134(makeModel134("label", "Label"));
