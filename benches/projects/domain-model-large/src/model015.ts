// Generated domain module 15 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model014Service, summarizeModel014 } from "./model014";
import type { Model014 } from "./model014";
import { Model008Service, summarizeModel008 } from "./model008";
import type { Model008 } from "./model008";

export type Model015Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model015Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model015 extends Entity<"model015"> {
  updatedAt?: number;
  name: string;
  status: Model015Status;
  tags: string[];
  lines: Model015Line[];
  owner?: { name: string; email?: string };
  parent?: Model014;
  related: Model008[];
}

export type Model015Event =
  | { kind: "created"; item: Model015 }
  | { kind: "renamed"; id: Id<"model015">; from: string; to: string }
  | { kind: "moved"; id: Id<"model015">; status: Model015Status }
  | { kind: "deleted"; id: Id<"model015">; reason?: string };

export type Model015Events = {
  change: Model015Event;
  error: { message: string; code: number };
};

export type Model015Numbers = KeysOfType<Model015Line, number>;
export type FrozenModel015 = DeepReadonly<Model015>;

export function describeModel015Event(event: Model015Event): string {
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

export function totalModel015(item: Model015): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel015(item: Model015): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel015(item), 3), (item.parent ? summarizeModel014(item.parent) : "-"), item.related.map(summarizeModel008).join(",")].join(" | ");
}

export class Model015Service extends Service<Model015, "model015"> {
  readonly events = new EventBus<Model015Events>();
  private readonly parents?: Model014Service;

  constructor(repository = new MemoryRepository<Model015, "model015">()) {
    super(repository);
  }

  validate(item: Model015): string[] {
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

  rename(id: Id<"model015">, to: string): Result<Model015> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model015 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model015">, status: Model015Status): Result<Model015Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model015">, patch: Patch<Pick<Model015, "name" | "tags">>): Model015 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model015Status, Model015[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model015Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model015">[]): Promise<Model015[]> {
    const found: Model015[] = [];
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

  linkedService(): Model014Service {
    return this.parents ?? new Model014Service();
  }
}

export function makeModel015(id: string, name: string): Model015 {
  return {
    id: id as Id<"model015">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 16, unit: "kg" }],
    related: [],
  };
}

export const model015Defaults: FrozenModel015 = makeModel015("default-15", "Default 15");
export const model015Label = summarizeModel015(makeModel015("label", "Label"));
