// Generated domain module 56 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model055Service, summarizeModel055 } from "./model055";
import type { Model055 } from "./model055";
import { Model049Service, summarizeModel049 } from "./model049";
import type { Model049 } from "./model049";

export type Model056Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model056Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model056 extends Entity<"model056"> {
  updatedAt?: number;
  name: string;
  status: Model056Status;
  tags: string[];
  lines: Model056Line[];
  owner?: { name: string; email?: string };
  parent?: Model055;
  related: Model049[];
}

export type Model056Event =
  | { kind: "created"; item: Model056 }
  | { kind: "renamed"; id: Id<"model056">; from: string; to: string }
  | { kind: "moved"; id: Id<"model056">; status: Model056Status }
  | { kind: "deleted"; id: Id<"model056">; reason?: string };

export type Model056Events = {
  change: Model056Event;
  error: { message: string; code: number };
};

export type Model056Numbers = KeysOfType<Model056Line, number>;
export type FrozenModel056 = DeepReadonly<Model056>;

export function describeModel056Event(event: Model056Event): string {
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

export function totalModel056(item: Model056): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel056(item: Model056): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel056(item), 3), (item.parent ? summarizeModel055(item.parent) : "-"), item.related.map(summarizeModel049).join(",")].join(" | ");
}

export class Model056Service extends Service<Model056, "model056"> {
  readonly events = new EventBus<Model056Events>();
  private readonly parents?: Model055Service;

  constructor(repository = new MemoryRepository<Model056, "model056">()) {
    super(repository);
  }

  validate(item: Model056): string[] {
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

  rename(id: Id<"model056">, to: string): Result<Model056> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model056 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model056">, status: Model056Status): Result<Model056Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model056">, patch: Patch<Pick<Model056, "name" | "tags">>): Model056 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model056Status, Model056[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model056Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model056">[]): Promise<Model056[]> {
    const found: Model056[] = [];
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

  linkedService(): Model055Service {
    return this.parents ?? new Model055Service();
  }
}

export function makeModel056(id: string, name: string): Model056 {
  return {
    id: id as Id<"model056">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 57, unit: "m" }],
    related: [],
  };
}

export const model056Defaults: FrozenModel056 = makeModel056("default-56", "Default 56");
export const model056Label = summarizeModel056(makeModel056("label", "Label"));
