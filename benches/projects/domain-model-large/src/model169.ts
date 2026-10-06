// Generated domain module 169 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model168Service, summarizeModel168 } from "./model168";
import type { Model168 } from "./model168";
import { Model162Service, summarizeModel162 } from "./model162";
import type { Model162 } from "./model162";

export type Model169Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model169Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model169 extends Entity<"model169"> {
  updatedAt?: number;
  name: string;
  status: Model169Status;
  tags: string[];
  lines: Model169Line[];
  owner?: { name: string; email?: string };
  parent?: Model168;
  related: Model162[];
}

export type Model169Event =
  | { kind: "created"; item: Model169 }
  | { kind: "renamed"; id: Id<"model169">; from: string; to: string }
  | { kind: "moved"; id: Id<"model169">; status: Model169Status }
  | { kind: "deleted"; id: Id<"model169">; reason?: string };

export type Model169Events = {
  change: Model169Event;
  error: { message: string; code: number };
};

export type Model169Numbers = KeysOfType<Model169Line, number>;
export type FrozenModel169 = DeepReadonly<Model169>;

export function describeModel169Event(event: Model169Event): string {
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

export function totalModel169(item: Model169): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel169(item: Model169): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel169(item), 3), (item.parent ? summarizeModel168(item.parent) : "-"), item.related.map(summarizeModel162).join(",")].join(" | ");
}

export class Model169Service extends Service<Model169, "model169"> {
  readonly events = new EventBus<Model169Events>();
  private readonly parents?: Model168Service;

  constructor(repository = new MemoryRepository<Model169, "model169">()) {
    super(repository);
  }

  validate(item: Model169): string[] {
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

  rename(id: Id<"model169">, to: string): Result<Model169> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model169 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model169">, status: Model169Status): Result<Model169Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model169">, patch: Patch<Pick<Model169, "name" | "tags">>): Model169 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model169Status, Model169[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model169Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model169">[]): Promise<Model169[]> {
    const found: Model169[] = [];
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

  linkedService(): Model168Service {
    return this.parents ?? new Model168Service();
  }
}

export function makeModel169(id: string, name: string): Model169 {
  return {
    id: id as Id<"model169">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 170, unit: "hour" }],
    related: [],
  };
}

export const model169Defaults: FrozenModel169 = makeModel169("default-169", "Default 169");
export const model169Label = summarizeModel169(makeModel169("label", "Label"));
