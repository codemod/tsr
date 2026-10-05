// Generated domain module 144 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model143Service, summarizeModel143 } from "./model143";
import type { Model143 } from "./model143";
import { Model137Service, summarizeModel137 } from "./model137";
import type { Model137 } from "./model137";

export type Model144Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model144Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model144 extends Entity<"model144"> {
  updatedAt?: number;
  name: string;
  status: Model144Status;
  tags: string[];
  lines: Model144Line[];
  owner?: { name: string; email?: string };
  parent?: Model143;
  related: Model137[];
}

export type Model144Event =
  | { kind: "created"; item: Model144 }
  | { kind: "renamed"; id: Id<"model144">; from: string; to: string }
  | { kind: "moved"; id: Id<"model144">; status: Model144Status }
  | { kind: "deleted"; id: Id<"model144">; reason?: string };

export type Model144Events = {
  change: Model144Event;
  error: { message: string; code: number };
};

export type Model144Numbers = KeysOfType<Model144Line, number>;
export type FrozenModel144 = DeepReadonly<Model144>;

export function describeModel144Event(event: Model144Event): string {
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

export function totalModel144(item: Model144): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel144(item: Model144): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel144(item), 3), (item.parent ? summarizeModel143(item.parent) : "-"), item.related.map(summarizeModel137).join(",")].join(" | ");
}

export class Model144Service extends Service<Model144, "model144"> {
  readonly events = new EventBus<Model144Events>();
  private readonly parents?: Model143Service;

  constructor(repository = new MemoryRepository<Model144, "model144">()) {
    super(repository);
  }

  validate(item: Model144): string[] {
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

  rename(id: Id<"model144">, to: string): Result<Model144> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model144 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model144">, status: Model144Status): Result<Model144Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model144">, patch: Patch<Pick<Model144, "name" | "tags">>): Model144 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model144Status, Model144[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model144Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model144">[]): Promise<Model144[]> {
    const found: Model144[] = [];
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

  linkedService(): Model143Service {
    return this.parents ?? new Model143Service();
  }
}

export function makeModel144(id: string, name: string): Model144 {
  return {
    id: id as Id<"model144">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 145, unit: "hour" }],
    related: [],
  };
}

export const model144Defaults: FrozenModel144 = makeModel144("default-144", "Default 144");
export const model144Label = summarizeModel144(makeModel144("label", "Label"));
