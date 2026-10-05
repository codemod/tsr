// Generated domain module 177 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model176Service, summarizeModel176 } from "./model176";
import type { Model176 } from "./model176";
import { Model170Service, summarizeModel170 } from "./model170";
import type { Model170 } from "./model170";

export type Model177Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model177Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model177 extends Entity<"model177"> {
  updatedAt?: number;
  name: string;
  status: Model177Status;
  tags: string[];
  lines: Model177Line[];
  owner?: { name: string; email?: string };
  parent?: Model176;
  related: Model170[];
}

export type Model177Event =
  | { kind: "created"; item: Model177 }
  | { kind: "renamed"; id: Id<"model177">; from: string; to: string }
  | { kind: "moved"; id: Id<"model177">; status: Model177Status }
  | { kind: "deleted"; id: Id<"model177">; reason?: string };

export type Model177Events = {
  change: Model177Event;
  error: { message: string; code: number };
};

export type Model177Numbers = KeysOfType<Model177Line, number>;
export type FrozenModel177 = DeepReadonly<Model177>;

export function describeModel177Event(event: Model177Event): string {
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

export function totalModel177(item: Model177): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel177(item: Model177): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel177(item), 3), (item.parent ? summarizeModel176(item.parent) : "-"), item.related.map(summarizeModel170).join(",")].join(" | ");
}

export class Model177Service extends Service<Model177, "model177"> {
  readonly events = new EventBus<Model177Events>();
  private readonly parents?: Model176Service;

  constructor(repository = new MemoryRepository<Model177, "model177">()) {
    super(repository);
  }

  validate(item: Model177): string[] {
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

  rename(id: Id<"model177">, to: string): Result<Model177> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model177 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model177">, status: Model177Status): Result<Model177Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model177">, patch: Patch<Pick<Model177, "name" | "tags">>): Model177 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model177Status, Model177[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model177Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model177">[]): Promise<Model177[]> {
    const found: Model177[] = [];
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

  linkedService(): Model176Service {
    return this.parents ?? new Model176Service();
  }
}

export function makeModel177(id: string, name: string): Model177 {
  return {
    id: id as Id<"model177">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 178, unit: "s" }],
    related: [],
  };
}

export const model177Defaults: FrozenModel177 = makeModel177("default-177", "Default 177");
export const model177Label = summarizeModel177(makeModel177("label", "Label"));
