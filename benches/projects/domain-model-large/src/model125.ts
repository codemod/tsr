// Generated domain module 125 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model124Service, summarizeModel124 } from "./model124";
import type { Model124 } from "./model124";
import { Model118Service, summarizeModel118 } from "./model118";
import type { Model118 } from "./model118";

export type Model125Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model125Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model125 extends Entity<"model125"> {
  updatedAt?: number;
  name: string;
  status: Model125Status;
  tags: string[];
  lines: Model125Line[];
  owner?: { name: string; email?: string };
  parent?: Model124;
  related: Model118[];
}

export type Model125Event =
  | { kind: "created"; item: Model125 }
  | { kind: "renamed"; id: Id<"model125">; from: string; to: string }
  | { kind: "moved"; id: Id<"model125">; status: Model125Status }
  | { kind: "deleted"; id: Id<"model125">; reason?: string };

export type Model125Events = {
  change: Model125Event;
  error: { message: string; code: number };
};

export type Model125Numbers = KeysOfType<Model125Line, number>;
export type FrozenModel125 = DeepReadonly<Model125>;

export function describeModel125Event(event: Model125Event): string {
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

export function totalModel125(item: Model125): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel125(item: Model125): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel125(item), 3), (item.parent ? summarizeModel124(item.parent) : "-"), item.related.map(summarizeModel118).join(",")].join(" | ");
}

export class Model125Service extends Service<Model125, "model125"> {
  readonly events = new EventBus<Model125Events>();
  private readonly parents?: Model124Service;

  constructor(repository = new MemoryRepository<Model125, "model125">()) {
    super(repository);
  }

  validate(item: Model125): string[] {
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

  rename(id: Id<"model125">, to: string): Result<Model125> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model125 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model125">, status: Model125Status): Result<Model125Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model125">, patch: Patch<Pick<Model125, "name" | "tags">>): Model125 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model125Status, Model125[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model125Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model125">[]): Promise<Model125[]> {
    const found: Model125[] = [];
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

  linkedService(): Model124Service {
    return this.parents ?? new Model124Service();
  }
}

export function makeModel125(id: string, name: string): Model125 {
  return {
    id: id as Id<"model125">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 126, unit: "kg" }],
    related: [],
  };
}

export const model125Defaults: FrozenModel125 = makeModel125("default-125", "Default 125");
export const model125Label = summarizeModel125(makeModel125("label", "Label"));
