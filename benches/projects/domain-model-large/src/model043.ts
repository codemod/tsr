// Generated domain module 43 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model042Service, summarizeModel042 } from "./model042";
import type { Model042 } from "./model042";
import { Model036Service, summarizeModel036 } from "./model036";
import type { Model036 } from "./model036";

export type Model043Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model043Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model043 extends Entity<"model043"> {
  updatedAt?: number;
  name: string;
  status: Model043Status;
  tags: string[];
  lines: Model043Line[];
  owner?: { name: string; email?: string };
  parent?: Model042;
  related: Model036[];
}

export type Model043Event =
  | { kind: "created"; item: Model043 }
  | { kind: "renamed"; id: Id<"model043">; from: string; to: string }
  | { kind: "moved"; id: Id<"model043">; status: Model043Status }
  | { kind: "deleted"; id: Id<"model043">; reason?: string };

export type Model043Events = {
  change: Model043Event;
  error: { message: string; code: number };
};

export type Model043Numbers = KeysOfType<Model043Line, number>;
export type FrozenModel043 = DeepReadonly<Model043>;

export function describeModel043Event(event: Model043Event): string {
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

export function totalModel043(item: Model043): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel043(item: Model043): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel043(item), 3), (item.parent ? summarizeModel042(item.parent) : "-"), item.related.map(summarizeModel036).join(",")].join(" | ");
}

export class Model043Service extends Service<Model043, "model043"> {
  readonly events = new EventBus<Model043Events>();
  private readonly parents?: Model042Service;

  constructor(repository = new MemoryRepository<Model043, "model043">()) {
    super(repository);
  }

  validate(item: Model043): string[] {
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

  rename(id: Id<"model043">, to: string): Result<Model043> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model043 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model043">, status: Model043Status): Result<Model043Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model043">, patch: Patch<Pick<Model043, "name" | "tags">>): Model043 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model043Status, Model043[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model043Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model043">[]): Promise<Model043[]> {
    const found: Model043[] = [];
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

  linkedService(): Model042Service {
    return this.parents ?? new Model042Service();
  }
}

export function makeModel043(id: string, name: string): Model043 {
  return {
    id: id as Id<"model043">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 44, unit: "item" }],
    related: [],
  };
}

export const model043Defaults: FrozenModel043 = makeModel043("default-43", "Default 43");
export const model043Label = summarizeModel043(makeModel043("label", "Label"));
