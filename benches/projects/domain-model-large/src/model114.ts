// Generated domain module 114 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model113Service, summarizeModel113 } from "./model113";
import type { Model113 } from "./model113";
import { Model107Service, summarizeModel107 } from "./model107";
import type { Model107 } from "./model107";

export type Model114Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model114Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model114 extends Entity<"model114"> {
  updatedAt?: number;
  name: string;
  status: Model114Status;
  tags: string[];
  lines: Model114Line[];
  owner?: { name: string; email?: string };
  parent?: Model113;
  related: Model107[];
}

export type Model114Event =
  | { kind: "created"; item: Model114 }
  | { kind: "renamed"; id: Id<"model114">; from: string; to: string }
  | { kind: "moved"; id: Id<"model114">; status: Model114Status }
  | { kind: "deleted"; id: Id<"model114">; reason?: string };

export type Model114Events = {
  change: Model114Event;
  error: { message: string; code: number };
};

export type Model114Numbers = KeysOfType<Model114Line, number>;
export type FrozenModel114 = DeepReadonly<Model114>;

export function describeModel114Event(event: Model114Event): string {
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

export function totalModel114(item: Model114): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel114(item: Model114): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel114(item), 3), (item.parent ? summarizeModel113(item.parent) : "-"), item.related.map(summarizeModel107).join(",")].join(" | ");
}

export class Model114Service extends Service<Model114, "model114"> {
  readonly events = new EventBus<Model114Events>();
  private readonly parents?: Model113Service;

  constructor(repository = new MemoryRepository<Model114, "model114">()) {
    super(repository);
  }

  validate(item: Model114): string[] {
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

  rename(id: Id<"model114">, to: string): Result<Model114> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model114 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model114">, status: Model114Status): Result<Model114Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model114">, patch: Patch<Pick<Model114, "name" | "tags">>): Model114 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model114Status, Model114[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model114Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model114">[]): Promise<Model114[]> {
    const found: Model114[] = [];
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

  linkedService(): Model113Service {
    return this.parents ?? new Model113Service();
  }
}

export function makeModel114(id: string, name: string): Model114 {
  return {
    id: id as Id<"model114">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 115, unit: "hour" }],
    related: [],
  };
}

export const model114Defaults: FrozenModel114 = makeModel114("default-114", "Default 114");
export const model114Label = summarizeModel114(makeModel114("label", "Label"));
