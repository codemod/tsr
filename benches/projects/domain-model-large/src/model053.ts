// Generated domain module 53 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model052Service, summarizeModel052 } from "./model052";
import type { Model052 } from "./model052";
import { Model046Service, summarizeModel046 } from "./model046";
import type { Model046 } from "./model046";

export type Model053Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model053Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model053 extends Entity<"model053"> {
  updatedAt?: number;
  name: string;
  status: Model053Status;
  tags: string[];
  lines: Model053Line[];
  owner?: { name: string; email?: string };
  parent?: Model052;
  related: Model046[];
}

export type Model053Event =
  | { kind: "created"; item: Model053 }
  | { kind: "renamed"; id: Id<"model053">; from: string; to: string }
  | { kind: "moved"; id: Id<"model053">; status: Model053Status }
  | { kind: "deleted"; id: Id<"model053">; reason?: string };

export type Model053Events = {
  change: Model053Event;
  error: { message: string; code: number };
};

export type Model053Numbers = KeysOfType<Model053Line, number>;
export type FrozenModel053 = DeepReadonly<Model053>;

export function describeModel053Event(event: Model053Event): string {
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

export function totalModel053(item: Model053): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel053(item: Model053): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel053(item), 3), (item.parent ? summarizeModel052(item.parent) : "-"), item.related.map(summarizeModel046).join(",")].join(" | ");
}

export class Model053Service extends Service<Model053, "model053"> {
  readonly events = new EventBus<Model053Events>();
  private readonly parents?: Model052Service;

  constructor(repository = new MemoryRepository<Model053, "model053">()) {
    super(repository);
  }

  validate(item: Model053): string[] {
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

  rename(id: Id<"model053">, to: string): Result<Model053> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model053 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model053">, status: Model053Status): Result<Model053Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model053">, patch: Patch<Pick<Model053, "name" | "tags">>): Model053 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model053Status, Model053[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model053Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model053">[]): Promise<Model053[]> {
    const found: Model053[] = [];
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

  linkedService(): Model052Service {
    return this.parents ?? new Model052Service();
  }
}

export function makeModel053(id: string, name: string): Model053 {
  return {
    id: id as Id<"model053">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 54, unit: "item" }],
    related: [],
  };
}

export const model053Defaults: FrozenModel053 = makeModel053("default-53", "Default 53");
export const model053Label = summarizeModel053(makeModel053("label", "Label"));
