// Generated domain module 111 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model110Service, summarizeModel110 } from "./model110";
import type { Model110 } from "./model110";
import { Model104Service, summarizeModel104 } from "./model104";
import type { Model104 } from "./model104";

export type Model111Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model111Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model111 extends Entity<"model111"> {
  updatedAt?: number;
  name: string;
  status: Model111Status;
  tags: string[];
  lines: Model111Line[];
  owner?: { name: string; email?: string };
  parent?: Model110;
  related: Model104[];
}

export type Model111Event =
  | { kind: "created"; item: Model111 }
  | { kind: "renamed"; id: Id<"model111">; from: string; to: string }
  | { kind: "moved"; id: Id<"model111">; status: Model111Status }
  | { kind: "deleted"; id: Id<"model111">; reason?: string };

export type Model111Events = {
  change: Model111Event;
  error: { message: string; code: number };
};

export type Model111Numbers = KeysOfType<Model111Line, number>;
export type FrozenModel111 = DeepReadonly<Model111>;

export function describeModel111Event(event: Model111Event): string {
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

export function totalModel111(item: Model111): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel111(item: Model111): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel111(item), 3), (item.parent ? summarizeModel110(item.parent) : "-"), item.related.map(summarizeModel104).join(",")].join(" | ");
}

export class Model111Service extends Service<Model111, "model111"> {
  readonly events = new EventBus<Model111Events>();
  private readonly parents?: Model110Service;

  constructor(repository = new MemoryRepository<Model111, "model111">()) {
    super(repository);
  }

  validate(item: Model111): string[] {
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

  rename(id: Id<"model111">, to: string): Result<Model111> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model111 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model111">, status: Model111Status): Result<Model111Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model111">, patch: Patch<Pick<Model111, "name" | "tags">>): Model111 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model111Status, Model111[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model111Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model111">[]): Promise<Model111[]> {
    const found: Model111[] = [];
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

  linkedService(): Model110Service {
    return this.parents ?? new Model110Service();
  }
}

export function makeModel111(id: string, name: string): Model111 {
  return {
    id: id as Id<"model111">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 112, unit: "m" }],
    related: [],
  };
}

export const model111Defaults: FrozenModel111 = makeModel111("default-111", "Default 111");
export const model111Label = summarizeModel111(makeModel111("label", "Label"));
