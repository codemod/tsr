// Generated domain module 160 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model159Service, summarizeModel159 } from "./model159";
import type { Model159 } from "./model159";
import { Model153Service, summarizeModel153 } from "./model153";
import type { Model153 } from "./model153";

export type Model160Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model160Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model160 extends Entity<"model160"> {
  updatedAt?: number;
  name: string;
  status: Model160Status;
  tags: string[];
  lines: Model160Line[];
  owner?: { name: string; email?: string };
  parent?: Model159;
  related: Model153[];
}

export type Model160Event =
  | { kind: "created"; item: Model160 }
  | { kind: "renamed"; id: Id<"model160">; from: string; to: string }
  | { kind: "moved"; id: Id<"model160">; status: Model160Status }
  | { kind: "deleted"; id: Id<"model160">; reason?: string };

export type Model160Events = {
  change: Model160Event;
  error: { message: string; code: number };
};

export type Model160Numbers = KeysOfType<Model160Line, number>;
export type FrozenModel160 = DeepReadonly<Model160>;

export function describeModel160Event(event: Model160Event): string {
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

export function totalModel160(item: Model160): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel160(item: Model160): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel160(item), 3), (item.parent ? summarizeModel159(item.parent) : "-"), item.related.map(summarizeModel153).join(",")].join(" | ");
}

export class Model160Service extends Service<Model160, "model160"> {
  readonly events = new EventBus<Model160Events>();
  private readonly parents?: Model159Service;

  constructor(repository = new MemoryRepository<Model160, "model160">()) {
    super(repository);
  }

  validate(item: Model160): string[] {
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

  rename(id: Id<"model160">, to: string): Result<Model160> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model160 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model160">, status: Model160Status): Result<Model160Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model160">, patch: Patch<Pick<Model160, "name" | "tags">>): Model160 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model160Status, Model160[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model160Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model160">[]): Promise<Model160[]> {
    const found: Model160[] = [];
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

  linkedService(): Model159Service {
    return this.parents ?? new Model159Service();
  }
}

export function makeModel160(id: string, name: string): Model160 {
  return {
    id: id as Id<"model160">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 161, unit: "kg" }],
    related: [],
  };
}

export const model160Defaults: FrozenModel160 = makeModel160("default-160", "Default 160");
export const model160Label = summarizeModel160(makeModel160("label", "Label"));
