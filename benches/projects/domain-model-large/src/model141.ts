// Generated domain module 141 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model140Service, summarizeModel140 } from "./model140";
import type { Model140 } from "./model140";
import { Model134Service, summarizeModel134 } from "./model134";
import type { Model134 } from "./model134";

export type Model141Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model141Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model141 extends Entity<"model141"> {
  updatedAt?: number;
  name: string;
  status: Model141Status;
  tags: string[];
  lines: Model141Line[];
  owner?: { name: string; email?: string };
  parent?: Model140;
  related: Model134[];
}

export type Model141Event =
  | { kind: "created"; item: Model141 }
  | { kind: "renamed"; id: Id<"model141">; from: string; to: string }
  | { kind: "moved"; id: Id<"model141">; status: Model141Status }
  | { kind: "deleted"; id: Id<"model141">; reason?: string };

export type Model141Events = {
  change: Model141Event;
  error: { message: string; code: number };
};

export type Model141Numbers = KeysOfType<Model141Line, number>;
export type FrozenModel141 = DeepReadonly<Model141>;

export function describeModel141Event(event: Model141Event): string {
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

export function totalModel141(item: Model141): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel141(item: Model141): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel141(item), 3), (item.parent ? summarizeModel140(item.parent) : "-"), item.related.map(summarizeModel134).join(",")].join(" | ");
}

export class Model141Service extends Service<Model141, "model141"> {
  readonly events = new EventBus<Model141Events>();
  private readonly parents?: Model140Service;

  constructor(repository = new MemoryRepository<Model141, "model141">()) {
    super(repository);
  }

  validate(item: Model141): string[] {
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

  rename(id: Id<"model141">, to: string): Result<Model141> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model141 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model141">, status: Model141Status): Result<Model141Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model141">, patch: Patch<Pick<Model141, "name" | "tags">>): Model141 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model141Status, Model141[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model141Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model141">[]): Promise<Model141[]> {
    const found: Model141[] = [];
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

  linkedService(): Model140Service {
    return this.parents ?? new Model140Service();
  }
}

export function makeModel141(id: string, name: string): Model141 {
  return {
    id: id as Id<"model141">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 142, unit: "m" }],
    related: [],
  };
}

export const model141Defaults: FrozenModel141 = makeModel141("default-141", "Default 141");
export const model141Label = summarizeModel141(makeModel141("label", "Label"));
