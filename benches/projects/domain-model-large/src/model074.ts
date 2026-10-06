// Generated domain module 74 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model073Service, summarizeModel073 } from "./model073";
import type { Model073 } from "./model073";
import { Model067Service, summarizeModel067 } from "./model067";
import type { Model067 } from "./model067";

export type Model074Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model074Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model074 extends Entity<"model074"> {
  updatedAt?: number;
  name: string;
  status: Model074Status;
  tags: string[];
  lines: Model074Line[];
  owner?: { name: string; email?: string };
  parent?: Model073;
  related: Model067[];
}

export type Model074Event =
  | { kind: "created"; item: Model074 }
  | { kind: "renamed"; id: Id<"model074">; from: string; to: string }
  | { kind: "moved"; id: Id<"model074">; status: Model074Status }
  | { kind: "deleted"; id: Id<"model074">; reason?: string };

export type Model074Events = {
  change: Model074Event;
  error: { message: string; code: number };
};

export type Model074Numbers = KeysOfType<Model074Line, number>;
export type FrozenModel074 = DeepReadonly<Model074>;

export function describeModel074Event(event: Model074Event): string {
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

export function totalModel074(item: Model074): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel074(item: Model074): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel074(item), 3), (item.parent ? summarizeModel073(item.parent) : "-"), item.related.map(summarizeModel067).join(",")].join(" | ");
}

export class Model074Service extends Service<Model074, "model074"> {
  readonly events = new EventBus<Model074Events>();
  private readonly parents?: Model073Service;

  constructor(repository = new MemoryRepository<Model074, "model074">()) {
    super(repository);
  }

  validate(item: Model074): string[] {
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

  rename(id: Id<"model074">, to: string): Result<Model074> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model074 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model074">, status: Model074Status): Result<Model074Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model074">, patch: Patch<Pick<Model074, "name" | "tags">>): Model074 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model074Status, Model074[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model074Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model074">[]): Promise<Model074[]> {
    const found: Model074[] = [];
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

  linkedService(): Model073Service {
    return this.parents ?? new Model073Service();
  }
}

export function makeModel074(id: string, name: string): Model074 {
  return {
    id: id as Id<"model074">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 75, unit: "hour" }],
    related: [],
  };
}

export const model074Defaults: FrozenModel074 = makeModel074("default-74", "Default 74");
export const model074Label = summarizeModel074(makeModel074("label", "Label"));
