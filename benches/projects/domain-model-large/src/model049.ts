// Generated domain module 49 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model048Service, summarizeModel048 } from "./model048";
import type { Model048 } from "./model048";
import { Model042Service, summarizeModel042 } from "./model042";
import type { Model042 } from "./model042";

export type Model049Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model049Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model049 extends Entity<"model049"> {
  updatedAt?: number;
  name: string;
  status: Model049Status;
  tags: string[];
  lines: Model049Line[];
  owner?: { name: string; email?: string };
  parent?: Model048;
  related: Model042[];
}

export type Model049Event =
  | { kind: "created"; item: Model049 }
  | { kind: "renamed"; id: Id<"model049">; from: string; to: string }
  | { kind: "moved"; id: Id<"model049">; status: Model049Status }
  | { kind: "deleted"; id: Id<"model049">; reason?: string };

export type Model049Events = {
  change: Model049Event;
  error: { message: string; code: number };
};

export type Model049Numbers = KeysOfType<Model049Line, number>;
export type FrozenModel049 = DeepReadonly<Model049>;

export function describeModel049Event(event: Model049Event): string {
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

export function totalModel049(item: Model049): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel049(item: Model049): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel049(item), 3), (item.parent ? summarizeModel048(item.parent) : "-"), item.related.map(summarizeModel042).join(",")].join(" | ");
}

export class Model049Service extends Service<Model049, "model049"> {
  readonly events = new EventBus<Model049Events>();
  private readonly parents?: Model048Service;

  constructor(repository = new MemoryRepository<Model049, "model049">()) {
    super(repository);
  }

  validate(item: Model049): string[] {
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

  rename(id: Id<"model049">, to: string): Result<Model049> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model049 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model049">, status: Model049Status): Result<Model049Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model049">, patch: Patch<Pick<Model049, "name" | "tags">>): Model049 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model049Status, Model049[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model049Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model049">[]): Promise<Model049[]> {
    const found: Model049[] = [];
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

  linkedService(): Model048Service {
    return this.parents ?? new Model048Service();
  }
}

export function makeModel049(id: string, name: string): Model049 {
  return {
    id: id as Id<"model049">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 50, unit: "hour" }],
    related: [],
  };
}

export const model049Defaults: FrozenModel049 = makeModel049("default-49", "Default 49");
export const model049Label = summarizeModel049(makeModel049("label", "Label"));
