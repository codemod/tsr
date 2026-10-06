// Generated domain module 131 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model130Service, summarizeModel130 } from "./model130";
import type { Model130 } from "./model130";
import { Model124Service, summarizeModel124 } from "./model124";
import type { Model124 } from "./model124";

export type Model131Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model131Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model131 extends Entity<"model131"> {
  updatedAt?: number;
  name: string;
  status: Model131Status;
  tags: string[];
  lines: Model131Line[];
  owner?: { name: string; email?: string };
  parent?: Model130;
  related: Model124[];
}

export type Model131Event =
  | { kind: "created"; item: Model131 }
  | { kind: "renamed"; id: Id<"model131">; from: string; to: string }
  | { kind: "moved"; id: Id<"model131">; status: Model131Status }
  | { kind: "deleted"; id: Id<"model131">; reason?: string };

export type Model131Events = {
  change: Model131Event;
  error: { message: string; code: number };
};

export type Model131Numbers = KeysOfType<Model131Line, number>;
export type FrozenModel131 = DeepReadonly<Model131>;

export function describeModel131Event(event: Model131Event): string {
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

export function totalModel131(item: Model131): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel131(item: Model131): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel131(item), 3), (item.parent ? summarizeModel130(item.parent) : "-"), item.related.map(summarizeModel124).join(",")].join(" | ");
}

export class Model131Service extends Service<Model131, "model131"> {
  readonly events = new EventBus<Model131Events>();
  private readonly parents?: Model130Service;

  constructor(repository = new MemoryRepository<Model131, "model131">()) {
    super(repository);
  }

  validate(item: Model131): string[] {
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

  rename(id: Id<"model131">, to: string): Result<Model131> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model131 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model131">, status: Model131Status): Result<Model131Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model131">, patch: Patch<Pick<Model131, "name" | "tags">>): Model131 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model131Status, Model131[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model131Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model131">[]): Promise<Model131[]> {
    const found: Model131[] = [];
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

  linkedService(): Model130Service {
    return this.parents ?? new Model130Service();
  }
}

export function makeModel131(id: string, name: string): Model131 {
  return {
    id: id as Id<"model131">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 132, unit: "m" }],
    related: [],
  };
}

export const model131Defaults: FrozenModel131 = makeModel131("default-131", "Default 131");
export const model131Label = summarizeModel131(makeModel131("label", "Label"));
