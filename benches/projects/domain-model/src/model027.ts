// Generated domain module 27 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model026Service, summarizeModel026 } from "./model026";
import type { Model026 } from "./model026";
import { Model020Service, summarizeModel020 } from "./model020";
import type { Model020 } from "./model020";

export type Model027Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model027Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model027 extends Entity<"model027"> {
  updatedAt?: number;
  name: string;
  status: Model027Status;
  tags: string[];
  lines: Model027Line[];
  owner?: { name: string; email?: string };
  parent?: Model026;
  related: Model020[];
}

export type Model027Event =
  | { kind: "created"; item: Model027 }
  | { kind: "renamed"; id: Id<"model027">; from: string; to: string }
  | { kind: "moved"; id: Id<"model027">; status: Model027Status }
  | { kind: "deleted"; id: Id<"model027">; reason?: string };

export type Model027Events = {
  change: Model027Event;
  error: { message: string; code: number };
};

export type Model027Numbers = KeysOfType<Model027Line, number>;
export type FrozenModel027 = DeepReadonly<Model027>;

export function describeModel027Event(event: Model027Event): string {
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

export function totalModel027(item: Model027): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel027(item: Model027): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel027(item), 3), (item.parent ? summarizeModel026(item.parent) : "-"), item.related.map(summarizeModel020).join(",")].join(" | ");
}

export class Model027Service extends Service<Model027, "model027"> {
  readonly events = new EventBus<Model027Events>();
  private readonly parents?: Model026Service;

  constructor(repository = new MemoryRepository<Model027, "model027">()) {
    super(repository);
  }

  validate(item: Model027): string[] {
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

  rename(id: Id<"model027">, to: string): Result<Model027> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model027 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model027">, status: Model027Status): Result<Model027Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model027">, patch: Patch<Pick<Model027, "name" | "tags">>): Model027 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model027Status, Model027[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model027Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model027">[]): Promise<Model027[]> {
    const found: Model027[] = [];
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

  linkedService(): Model026Service {
    return this.parents ?? new Model026Service();
  }
}

export function makeModel027(id: string, name: string): Model027 {
  return {
    id: id as Id<"model027">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 28, unit: "s" }],
    related: [],
  };
}

export const model027Defaults: FrozenModel027 = makeModel027("default-27", "Default 27");
export const model027Label = summarizeModel027(makeModel027("label", "Label"));
