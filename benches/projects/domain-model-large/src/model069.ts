// Generated domain module 69 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model068Service, summarizeModel068 } from "./model068";
import type { Model068 } from "./model068";
import { Model062Service, summarizeModel062 } from "./model062";
import type { Model062 } from "./model062";

export type Model069Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model069Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model069 extends Entity<"model069"> {
  updatedAt?: number;
  name: string;
  status: Model069Status;
  tags: string[];
  lines: Model069Line[];
  owner?: { name: string; email?: string };
  parent?: Model068;
  related: Model062[];
}

export type Model069Event =
  | { kind: "created"; item: Model069 }
  | { kind: "renamed"; id: Id<"model069">; from: string; to: string }
  | { kind: "moved"; id: Id<"model069">; status: Model069Status }
  | { kind: "deleted"; id: Id<"model069">; reason?: string };

export type Model069Events = {
  change: Model069Event;
  error: { message: string; code: number };
};

export type Model069Numbers = KeysOfType<Model069Line, number>;
export type FrozenModel069 = DeepReadonly<Model069>;

export function describeModel069Event(event: Model069Event): string {
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

export function totalModel069(item: Model069): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel069(item: Model069): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel069(item), 3), (item.parent ? summarizeModel068(item.parent) : "-"), item.related.map(summarizeModel062).join(",")].join(" | ");
}

export class Model069Service extends Service<Model069, "model069"> {
  readonly events = new EventBus<Model069Events>();
  private readonly parents?: Model068Service;

  constructor(repository = new MemoryRepository<Model069, "model069">()) {
    super(repository);
  }

  validate(item: Model069): string[] {
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

  rename(id: Id<"model069">, to: string): Result<Model069> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model069 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model069">, status: Model069Status): Result<Model069Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model069">, patch: Patch<Pick<Model069, "name" | "tags">>): Model069 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model069Status, Model069[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model069Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model069">[]): Promise<Model069[]> {
    const found: Model069[] = [];
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

  linkedService(): Model068Service {
    return this.parents ?? new Model068Service();
  }
}

export function makeModel069(id: string, name: string): Model069 {
  return {
    id: id as Id<"model069">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 70, unit: "hour" }],
    related: [],
  };
}

export const model069Defaults: FrozenModel069 = makeModel069("default-69", "Default 69");
export const model069Label = summarizeModel069(makeModel069("label", "Label"));
