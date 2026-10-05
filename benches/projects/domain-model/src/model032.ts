// Generated domain module 32 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model031Service, summarizeModel031 } from "./model031";
import type { Model031 } from "./model031";
import { Model025Service, summarizeModel025 } from "./model025";
import type { Model025 } from "./model025";

export type Model032Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model032Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model032 extends Entity<"model032"> {
  updatedAt?: number;
  name: string;
  status: Model032Status;
  tags: string[];
  lines: Model032Line[];
  owner?: { name: string; email?: string };
  parent?: Model031;
  related: Model025[];
}

export type Model032Event =
  | { kind: "created"; item: Model032 }
  | { kind: "renamed"; id: Id<"model032">; from: string; to: string }
  | { kind: "moved"; id: Id<"model032">; status: Model032Status }
  | { kind: "deleted"; id: Id<"model032">; reason?: string };

export type Model032Events = {
  change: Model032Event;
  error: { message: string; code: number };
};

export type Model032Numbers = KeysOfType<Model032Line, number>;
export type FrozenModel032 = DeepReadonly<Model032>;

export function describeModel032Event(event: Model032Event): string {
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

export function totalModel032(item: Model032): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel032(item: Model032): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel032(item), 3), (item.parent ? summarizeModel031(item.parent) : "-"), item.related.map(summarizeModel025).join(",")].join(" | ");
}

export class Model032Service extends Service<Model032, "model032"> {
  readonly events = new EventBus<Model032Events>();
  private readonly parents?: Model031Service;

  constructor(repository = new MemoryRepository<Model032, "model032">()) {
    super(repository);
  }

  validate(item: Model032): string[] {
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

  rename(id: Id<"model032">, to: string): Result<Model032> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model032 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model032">, status: Model032Status): Result<Model032Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model032">, patch: Patch<Pick<Model032, "name" | "tags">>): Model032 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model032Status, Model032[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model032Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model032">[]): Promise<Model032[]> {
    const found: Model032[] = [];
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

  linkedService(): Model031Service {
    return this.parents ?? new Model031Service();
  }
}

export function makeModel032(id: string, name: string): Model032 {
  return {
    id: id as Id<"model032">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 33, unit: "s" }],
    related: [],
  };
}

export const model032Defaults: FrozenModel032 = makeModel032("default-32", "Default 32");
export const model032Label = summarizeModel032(makeModel032("label", "Label"));
