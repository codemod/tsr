// Generated domain module 21 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model020Service, summarizeModel020 } from "./model020";
import type { Model020 } from "./model020";
import { Model014Service, summarizeModel014 } from "./model014";
import type { Model014 } from "./model014";

export type Model021Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model021Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model021 extends Entity<"model021"> {
  updatedAt?: number;
  name: string;
  status: Model021Status;
  tags: string[];
  lines: Model021Line[];
  owner?: { name: string; email?: string };
  parent?: Model020;
  related: Model014[];
}

export type Model021Event =
  | { kind: "created"; item: Model021 }
  | { kind: "renamed"; id: Id<"model021">; from: string; to: string }
  | { kind: "moved"; id: Id<"model021">; status: Model021Status }
  | { kind: "deleted"; id: Id<"model021">; reason?: string };

export type Model021Events = {
  change: Model021Event;
  error: { message: string; code: number };
};

export type Model021Numbers = KeysOfType<Model021Line, number>;
export type FrozenModel021 = DeepReadonly<Model021>;

export function describeModel021Event(event: Model021Event): string {
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

export function totalModel021(item: Model021): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel021(item: Model021): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel021(item), 3), (item.parent ? summarizeModel020(item.parent) : "-"), item.related.map(summarizeModel014).join(",")].join(" | ");
}

export class Model021Service extends Service<Model021, "model021"> {
  readonly events = new EventBus<Model021Events>();
  private readonly parents?: Model020Service;

  constructor(repository = new MemoryRepository<Model021, "model021">()) {
    super(repository);
  }

  validate(item: Model021): string[] {
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

  rename(id: Id<"model021">, to: string): Result<Model021> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model021 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model021">, status: Model021Status): Result<Model021Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model021">, patch: Patch<Pick<Model021, "name" | "tags">>): Model021 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model021Status, Model021[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model021Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model021">[]): Promise<Model021[]> {
    const found: Model021[] = [];
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

  linkedService(): Model020Service {
    return this.parents ?? new Model020Service();
  }
}

export function makeModel021(id: string, name: string): Model021 {
  return {
    id: id as Id<"model021">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 22, unit: "m" }],
    related: [],
  };
}

export const model021Defaults: FrozenModel021 = makeModel021("default-21", "Default 21");
export const model021Label = summarizeModel021(makeModel021("label", "Label"));
