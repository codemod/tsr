// Generated domain module 193 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model192Service, summarizeModel192 } from "./model192";
import type { Model192 } from "./model192";
import { Model186Service, summarizeModel186 } from "./model186";
import type { Model186 } from "./model186";

export type Model193Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model193Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model193 extends Entity<"model193"> {
  updatedAt?: number;
  name: string;
  status: Model193Status;
  tags: string[];
  lines: Model193Line[];
  owner?: { name: string; email?: string };
  parent?: Model192;
  related: Model186[];
}

export type Model193Event =
  | { kind: "created"; item: Model193 }
  | { kind: "renamed"; id: Id<"model193">; from: string; to: string }
  | { kind: "moved"; id: Id<"model193">; status: Model193Status }
  | { kind: "deleted"; id: Id<"model193">; reason?: string };

export type Model193Events = {
  change: Model193Event;
  error: { message: string; code: number };
};

export type Model193Numbers = KeysOfType<Model193Line, number>;
export type FrozenModel193 = DeepReadonly<Model193>;

export function describeModel193Event(event: Model193Event): string {
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

export function totalModel193(item: Model193): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel193(item: Model193): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel193(item), 3), (item.parent ? summarizeModel192(item.parent) : "-"), item.related.map(summarizeModel186).join(",")].join(" | ");
}

export class Model193Service extends Service<Model193, "model193"> {
  readonly events = new EventBus<Model193Events>();
  private readonly parents?: Model192Service;

  constructor(repository = new MemoryRepository<Model193, "model193">()) {
    super(repository);
  }

  validate(item: Model193): string[] {
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

  rename(id: Id<"model193">, to: string): Result<Model193> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model193 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model193">, status: Model193Status): Result<Model193Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model193">, patch: Patch<Pick<Model193, "name" | "tags">>): Model193 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model193Status, Model193[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model193Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model193">[]): Promise<Model193[]> {
    const found: Model193[] = [];
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

  linkedService(): Model192Service {
    return this.parents ?? new Model192Service();
  }
}

export function makeModel193(id: string, name: string): Model193 {
  return {
    id: id as Id<"model193">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 194, unit: "item" }],
    related: [],
  };
}

export const model193Defaults: FrozenModel193 = makeModel193("default-193", "Default 193");
export const model193Label = summarizeModel193(makeModel193("label", "Label"));
