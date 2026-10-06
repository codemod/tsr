// Generated domain module 137 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model136Service, summarizeModel136 } from "./model136";
import type { Model136 } from "./model136";
import { Model130Service, summarizeModel130 } from "./model130";
import type { Model130 } from "./model130";

export type Model137Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model137Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model137 extends Entity<"model137"> {
  updatedAt?: number;
  name: string;
  status: Model137Status;
  tags: string[];
  lines: Model137Line[];
  owner?: { name: string; email?: string };
  parent?: Model136;
  related: Model130[];
}

export type Model137Event =
  | { kind: "created"; item: Model137 }
  | { kind: "renamed"; id: Id<"model137">; from: string; to: string }
  | { kind: "moved"; id: Id<"model137">; status: Model137Status }
  | { kind: "deleted"; id: Id<"model137">; reason?: string };

export type Model137Events = {
  change: Model137Event;
  error: { message: string; code: number };
};

export type Model137Numbers = KeysOfType<Model137Line, number>;
export type FrozenModel137 = DeepReadonly<Model137>;

export function describeModel137Event(event: Model137Event): string {
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

export function totalModel137(item: Model137): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel137(item: Model137): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel137(item), 3), (item.parent ? summarizeModel136(item.parent) : "-"), item.related.map(summarizeModel130).join(",")].join(" | ");
}

export class Model137Service extends Service<Model137, "model137"> {
  readonly events = new EventBus<Model137Events>();
  private readonly parents?: Model136Service;

  constructor(repository = new MemoryRepository<Model137, "model137">()) {
    super(repository);
  }

  validate(item: Model137): string[] {
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

  rename(id: Id<"model137">, to: string): Result<Model137> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model137 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model137">, status: Model137Status): Result<Model137Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model137">, patch: Patch<Pick<Model137, "name" | "tags">>): Model137 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model137Status, Model137[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model137Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model137">[]): Promise<Model137[]> {
    const found: Model137[] = [];
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

  linkedService(): Model136Service {
    return this.parents ?? new Model136Service();
  }
}

export function makeModel137(id: string, name: string): Model137 {
  return {
    id: id as Id<"model137">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 138, unit: "s" }],
    related: [],
  };
}

export const model137Defaults: FrozenModel137 = makeModel137("default-137", "Default 137");
export const model137Label = summarizeModel137(makeModel137("label", "Label"));
