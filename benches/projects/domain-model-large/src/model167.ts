// Generated domain module 167 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model166Service, summarizeModel166 } from "./model166";
import type { Model166 } from "./model166";
import { Model160Service, summarizeModel160 } from "./model160";
import type { Model160 } from "./model160";

export type Model167Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model167Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model167 extends Entity<"model167"> {
  updatedAt?: number;
  name: string;
  status: Model167Status;
  tags: string[];
  lines: Model167Line[];
  owner?: { name: string; email?: string };
  parent?: Model166;
  related: Model160[];
}

export type Model167Event =
  | { kind: "created"; item: Model167 }
  | { kind: "renamed"; id: Id<"model167">; from: string; to: string }
  | { kind: "moved"; id: Id<"model167">; status: Model167Status }
  | { kind: "deleted"; id: Id<"model167">; reason?: string };

export type Model167Events = {
  change: Model167Event;
  error: { message: string; code: number };
};

export type Model167Numbers = KeysOfType<Model167Line, number>;
export type FrozenModel167 = DeepReadonly<Model167>;

export function describeModel167Event(event: Model167Event): string {
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

export function totalModel167(item: Model167): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel167(item: Model167): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel167(item), 3), (item.parent ? summarizeModel166(item.parent) : "-"), item.related.map(summarizeModel160).join(",")].join(" | ");
}

export class Model167Service extends Service<Model167, "model167"> {
  readonly events = new EventBus<Model167Events>();
  private readonly parents?: Model166Service;

  constructor(repository = new MemoryRepository<Model167, "model167">()) {
    super(repository);
  }

  validate(item: Model167): string[] {
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

  rename(id: Id<"model167">, to: string): Result<Model167> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model167 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model167">, status: Model167Status): Result<Model167Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model167">, patch: Patch<Pick<Model167, "name" | "tags">>): Model167 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model167Status, Model167[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model167Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model167">[]): Promise<Model167[]> {
    const found: Model167[] = [];
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

  linkedService(): Model166Service {
    return this.parents ?? new Model166Service();
  }
}

export function makeModel167(id: string, name: string): Model167 {
  return {
    id: id as Id<"model167">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 168, unit: "s" }],
    related: [],
  };
}

export const model167Defaults: FrozenModel167 = makeModel167("default-167", "Default 167");
export const model167Label = summarizeModel167(makeModel167("label", "Label"));
