// Generated domain module 82 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model081Service, summarizeModel081 } from "./model081";
import type { Model081 } from "./model081";
import { Model075Service, summarizeModel075 } from "./model075";
import type { Model075 } from "./model075";

export type Model082Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model082Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model082 extends Entity<"model082"> {
  updatedAt?: number;
  name: string;
  status: Model082Status;
  tags: string[];
  lines: Model082Line[];
  owner?: { name: string; email?: string };
  parent?: Model081;
  related: Model075[];
}

export type Model082Event =
  | { kind: "created"; item: Model082 }
  | { kind: "renamed"; id: Id<"model082">; from: string; to: string }
  | { kind: "moved"; id: Id<"model082">; status: Model082Status }
  | { kind: "deleted"; id: Id<"model082">; reason?: string };

export type Model082Events = {
  change: Model082Event;
  error: { message: string; code: number };
};

export type Model082Numbers = KeysOfType<Model082Line, number>;
export type FrozenModel082 = DeepReadonly<Model082>;

export function describeModel082Event(event: Model082Event): string {
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

export function totalModel082(item: Model082): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel082(item: Model082): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel082(item), 3), (item.parent ? summarizeModel081(item.parent) : "-"), item.related.map(summarizeModel075).join(",")].join(" | ");
}

export class Model082Service extends Service<Model082, "model082"> {
  readonly events = new EventBus<Model082Events>();
  private readonly parents?: Model081Service;

  constructor(repository = new MemoryRepository<Model082, "model082">()) {
    super(repository);
  }

  validate(item: Model082): string[] {
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

  rename(id: Id<"model082">, to: string): Result<Model082> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model082 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model082">, status: Model082Status): Result<Model082Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model082">, patch: Patch<Pick<Model082, "name" | "tags">>): Model082 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model082Status, Model082[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model082Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model082">[]): Promise<Model082[]> {
    const found: Model082[] = [];
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

  linkedService(): Model081Service {
    return this.parents ?? new Model081Service();
  }
}

export function makeModel082(id: string, name: string): Model082 {
  return {
    id: id as Id<"model082">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 83, unit: "s" }],
    related: [],
  };
}

export const model082Defaults: FrozenModel082 = makeModel082("default-82", "Default 82");
export const model082Label = summarizeModel082(makeModel082("label", "Label"));
