// Generated domain module 8 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model007Service, summarizeModel007 } from "./model007";
import type { Model007 } from "./model007";
import { Model001Service, summarizeModel001 } from "./model001";
import type { Model001 } from "./model001";

export type Model008Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model008Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model008 extends Entity<"model008"> {
  updatedAt?: number;
  name: string;
  status: Model008Status;
  tags: string[];
  lines: Model008Line[];
  owner?: { name: string; email?: string };
  parent?: Model007;
  related: Model001[];
}

export type Model008Event =
  | { kind: "created"; item: Model008 }
  | { kind: "renamed"; id: Id<"model008">; from: string; to: string }
  | { kind: "moved"; id: Id<"model008">; status: Model008Status }
  | { kind: "deleted"; id: Id<"model008">; reason?: string };

export type Model008Events = {
  change: Model008Event;
  error: { message: string; code: number };
};

export type Model008Numbers = KeysOfType<Model008Line, number>;
export type FrozenModel008 = DeepReadonly<Model008>;

export function describeModel008Event(event: Model008Event): string {
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

export function totalModel008(item: Model008): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel008(item: Model008): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel008(item), 3), (item.parent ? summarizeModel007(item.parent) : "-"), item.related.map(summarizeModel001).join(",")].join(" | ");
}

export class Model008Service extends Service<Model008, "model008"> {
  readonly events = new EventBus<Model008Events>();
  private readonly parents?: Model007Service;

  constructor(repository = new MemoryRepository<Model008, "model008">()) {
    super(repository);
  }

  validate(item: Model008): string[] {
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

  rename(id: Id<"model008">, to: string): Result<Model008> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model008 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model008">, status: Model008Status): Result<Model008Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model008">, patch: Patch<Pick<Model008, "name" | "tags">>): Model008 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model008Status, Model008[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model008Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model008">[]): Promise<Model008[]> {
    const found: Model008[] = [];
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

  linkedService(): Model007Service {
    return this.parents ?? new Model007Service();
  }
}

export function makeModel008(id: string, name: string): Model008 {
  return {
    id: id as Id<"model008">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 9, unit: "item" }],
    related: [],
  };
}

export const model008Defaults: FrozenModel008 = makeModel008("default-8", "Default 8");
export const model008Label = summarizeModel008(makeModel008("label", "Label"));
