// Generated domain module 11 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model010Service, summarizeModel010 } from "./model010";
import type { Model010 } from "./model010";
import { Model004Service, summarizeModel004 } from "./model004";
import type { Model004 } from "./model004";

export type Model011Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model011Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model011 extends Entity<"model011"> {
  updatedAt?: number;
  name: string;
  status: Model011Status;
  tags: string[];
  lines: Model011Line[];
  owner?: { name: string; email?: string };
  parent?: Model010;
  related: Model004[];
}

export type Model011Event =
  | { kind: "created"; item: Model011 }
  | { kind: "renamed"; id: Id<"model011">; from: string; to: string }
  | { kind: "moved"; id: Id<"model011">; status: Model011Status }
  | { kind: "deleted"; id: Id<"model011">; reason?: string };

export type Model011Events = {
  change: Model011Event;
  error: { message: string; code: number };
};

export type Model011Numbers = KeysOfType<Model011Line, number>;
export type FrozenModel011 = DeepReadonly<Model011>;

export function describeModel011Event(event: Model011Event): string {
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

export function totalModel011(item: Model011): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel011(item: Model011): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel011(item), 3), (item.parent ? summarizeModel010(item.parent) : "-"), item.related.map(summarizeModel004).join(",")].join(" | ");
}

export class Model011Service extends Service<Model011, "model011"> {
  readonly events = new EventBus<Model011Events>();
  private readonly parents?: Model010Service;

  constructor(repository = new MemoryRepository<Model011, "model011">()) {
    super(repository);
  }

  validate(item: Model011): string[] {
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

  rename(id: Id<"model011">, to: string): Result<Model011> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model011 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model011">, status: Model011Status): Result<Model011Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model011">, patch: Patch<Pick<Model011, "name" | "tags">>): Model011 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model011Status, Model011[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model011Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model011">[]): Promise<Model011[]> {
    const found: Model011[] = [];
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

  linkedService(): Model010Service {
    return this.parents ?? new Model010Service();
  }
}

export function makeModel011(id: string, name: string): Model011 {
  return {
    id: id as Id<"model011">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 12, unit: "m" }],
    related: [],
  };
}

export const model011Defaults: FrozenModel011 = makeModel011("default-11", "Default 11");
export const model011Label = summarizeModel011(makeModel011("label", "Label"));
