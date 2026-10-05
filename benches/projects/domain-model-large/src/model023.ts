// Generated domain module 23 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model022Service, summarizeModel022 } from "./model022";
import type { Model022 } from "./model022";
import { Model016Service, summarizeModel016 } from "./model016";
import type { Model016 } from "./model016";

export type Model023Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model023Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model023 extends Entity<"model023"> {
  updatedAt?: number;
  name: string;
  status: Model023Status;
  tags: string[];
  lines: Model023Line[];
  owner?: { name: string; email?: string };
  parent?: Model022;
  related: Model016[];
}

export type Model023Event =
  | { kind: "created"; item: Model023 }
  | { kind: "renamed"; id: Id<"model023">; from: string; to: string }
  | { kind: "moved"; id: Id<"model023">; status: Model023Status }
  | { kind: "deleted"; id: Id<"model023">; reason?: string };

export type Model023Events = {
  change: Model023Event;
  error: { message: string; code: number };
};

export type Model023Numbers = KeysOfType<Model023Line, number>;
export type FrozenModel023 = DeepReadonly<Model023>;

export function describeModel023Event(event: Model023Event): string {
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

export function totalModel023(item: Model023): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel023(item: Model023): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel023(item), 3), (item.parent ? summarizeModel022(item.parent) : "-"), item.related.map(summarizeModel016).join(",")].join(" | ");
}

export class Model023Service extends Service<Model023, "model023"> {
  readonly events = new EventBus<Model023Events>();
  private readonly parents?: Model022Service;

  constructor(repository = new MemoryRepository<Model023, "model023">()) {
    super(repository);
  }

  validate(item: Model023): string[] {
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

  rename(id: Id<"model023">, to: string): Result<Model023> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model023 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model023">, status: Model023Status): Result<Model023Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model023">, patch: Patch<Pick<Model023, "name" | "tags">>): Model023 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model023Status, Model023[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model023Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model023">[]): Promise<Model023[]> {
    const found: Model023[] = [];
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

  linkedService(): Model022Service {
    return this.parents ?? new Model022Service();
  }
}

export function makeModel023(id: string, name: string): Model023 {
  return {
    id: id as Id<"model023">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 24, unit: "item" }],
    related: [],
  };
}

export const model023Defaults: FrozenModel023 = makeModel023("default-23", "Default 23");
export const model023Label = summarizeModel023(makeModel023("label", "Label"));
