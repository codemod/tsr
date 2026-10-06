// Generated domain module 122 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model121Service, summarizeModel121 } from "./model121";
import type { Model121 } from "./model121";
import { Model115Service, summarizeModel115 } from "./model115";
import type { Model115 } from "./model115";

export type Model122Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model122Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model122 extends Entity<"model122"> {
  updatedAt?: number;
  name: string;
  status: Model122Status;
  tags: string[];
  lines: Model122Line[];
  owner?: { name: string; email?: string };
  parent?: Model121;
  related: Model115[];
}

export type Model122Event =
  | { kind: "created"; item: Model122 }
  | { kind: "renamed"; id: Id<"model122">; from: string; to: string }
  | { kind: "moved"; id: Id<"model122">; status: Model122Status }
  | { kind: "deleted"; id: Id<"model122">; reason?: string };

export type Model122Events = {
  change: Model122Event;
  error: { message: string; code: number };
};

export type Model122Numbers = KeysOfType<Model122Line, number>;
export type FrozenModel122 = DeepReadonly<Model122>;

export function describeModel122Event(event: Model122Event): string {
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

export function totalModel122(item: Model122): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel122(item: Model122): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel122(item), 3), (item.parent ? summarizeModel121(item.parent) : "-"), item.related.map(summarizeModel115).join(",")].join(" | ");
}

export class Model122Service extends Service<Model122, "model122"> {
  readonly events = new EventBus<Model122Events>();
  private readonly parents?: Model121Service;

  constructor(repository = new MemoryRepository<Model122, "model122">()) {
    super(repository);
  }

  validate(item: Model122): string[] {
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

  rename(id: Id<"model122">, to: string): Result<Model122> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model122 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model122">, status: Model122Status): Result<Model122Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model122">, patch: Patch<Pick<Model122, "name" | "tags">>): Model122 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model122Status, Model122[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model122Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model122">[]): Promise<Model122[]> {
    const found: Model122[] = [];
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

  linkedService(): Model121Service {
    return this.parents ?? new Model121Service();
  }
}

export function makeModel122(id: string, name: string): Model122 {
  return {
    id: id as Id<"model122">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 123, unit: "s" }],
    related: [],
  };
}

export const model122Defaults: FrozenModel122 = makeModel122("default-122", "Default 122");
export const model122Label = summarizeModel122(makeModel122("label", "Label"));
