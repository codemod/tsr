// Generated domain module 31 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model030Service, summarizeModel030 } from "./model030";
import type { Model030 } from "./model030";
import { Model024Service, summarizeModel024 } from "./model024";
import type { Model024 } from "./model024";

export type Model031Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model031Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model031 extends Entity<"model031"> {
  updatedAt?: number;
  name: string;
  status: Model031Status;
  tags: string[];
  lines: Model031Line[];
  owner?: { name: string; email?: string };
  parent?: Model030;
  related: Model024[];
}

export type Model031Event =
  | { kind: "created"; item: Model031 }
  | { kind: "renamed"; id: Id<"model031">; from: string; to: string }
  | { kind: "moved"; id: Id<"model031">; status: Model031Status }
  | { kind: "deleted"; id: Id<"model031">; reason?: string };

export type Model031Events = {
  change: Model031Event;
  error: { message: string; code: number };
};

export type Model031Numbers = KeysOfType<Model031Line, number>;
export type FrozenModel031 = DeepReadonly<Model031>;

export function describeModel031Event(event: Model031Event): string {
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

export function totalModel031(item: Model031): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel031(item: Model031): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel031(item), 3), (item.parent ? summarizeModel030(item.parent) : "-"), item.related.map(summarizeModel024).join(",")].join(" | ");
}

export class Model031Service extends Service<Model031, "model031"> {
  readonly events = new EventBus<Model031Events>();
  private readonly parents?: Model030Service;

  constructor(repository = new MemoryRepository<Model031, "model031">()) {
    super(repository);
  }

  validate(item: Model031): string[] {
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

  rename(id: Id<"model031">, to: string): Result<Model031> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model031 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model031">, status: Model031Status): Result<Model031Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model031">, patch: Patch<Pick<Model031, "name" | "tags">>): Model031 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model031Status, Model031[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model031Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model031">[]): Promise<Model031[]> {
    const found: Model031[] = [];
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

  linkedService(): Model030Service {
    return this.parents ?? new Model030Service();
  }
}

export function makeModel031(id: string, name: string): Model031 {
  return {
    id: id as Id<"model031">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 32, unit: "m" }],
    related: [],
  };
}

export const model031Defaults: FrozenModel031 = makeModel031("default-31", "Default 31");
export const model031Label = summarizeModel031(makeModel031("label", "Label"));
