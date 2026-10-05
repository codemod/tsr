// Generated domain module 46 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model045Service, summarizeModel045 } from "./model045";
import type { Model045 } from "./model045";
import { Model039Service, summarizeModel039 } from "./model039";
import type { Model039 } from "./model039";

export type Model046Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model046Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model046 extends Entity<"model046"> {
  updatedAt?: number;
  name: string;
  status: Model046Status;
  tags: string[];
  lines: Model046Line[];
  owner?: { name: string; email?: string };
  parent?: Model045;
  related: Model039[];
}

export type Model046Event =
  | { kind: "created"; item: Model046 }
  | { kind: "renamed"; id: Id<"model046">; from: string; to: string }
  | { kind: "moved"; id: Id<"model046">; status: Model046Status }
  | { kind: "deleted"; id: Id<"model046">; reason?: string };

export type Model046Events = {
  change: Model046Event;
  error: { message: string; code: number };
};

export type Model046Numbers = KeysOfType<Model046Line, number>;
export type FrozenModel046 = DeepReadonly<Model046>;

export function describeModel046Event(event: Model046Event): string {
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

export function totalModel046(item: Model046): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel046(item: Model046): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel046(item), 3), (item.parent ? summarizeModel045(item.parent) : "-"), item.related.map(summarizeModel039).join(",")].join(" | ");
}

export class Model046Service extends Service<Model046, "model046"> {
  readonly events = new EventBus<Model046Events>();
  private readonly parents?: Model045Service;

  constructor(repository = new MemoryRepository<Model046, "model046">()) {
    super(repository);
  }

  validate(item: Model046): string[] {
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

  rename(id: Id<"model046">, to: string): Result<Model046> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model046 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model046">, status: Model046Status): Result<Model046Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model046">, patch: Patch<Pick<Model046, "name" | "tags">>): Model046 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model046Status, Model046[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model046Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model046">[]): Promise<Model046[]> {
    const found: Model046[] = [];
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

  linkedService(): Model045Service {
    return this.parents ?? new Model045Service();
  }
}

export function makeModel046(id: string, name: string): Model046 {
  return {
    id: id as Id<"model046">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 47, unit: "m" }],
    related: [],
  };
}

export const model046Defaults: FrozenModel046 = makeModel046("default-46", "Default 46");
export const model046Label = summarizeModel046(makeModel046("label", "Label"));
