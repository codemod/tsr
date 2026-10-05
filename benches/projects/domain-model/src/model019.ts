// Generated domain module 19 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model018Service, summarizeModel018 } from "./model018";
import type { Model018 } from "./model018";
import { Model012Service, summarizeModel012 } from "./model012";
import type { Model012 } from "./model012";

export type Model019Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model019Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model019 extends Entity<"model019"> {
  updatedAt?: number;
  name: string;
  status: Model019Status;
  tags: string[];
  lines: Model019Line[];
  owner?: { name: string; email?: string };
  parent?: Model018;
  related: Model012[];
}

export type Model019Event =
  | { kind: "created"; item: Model019 }
  | { kind: "renamed"; id: Id<"model019">; from: string; to: string }
  | { kind: "moved"; id: Id<"model019">; status: Model019Status }
  | { kind: "deleted"; id: Id<"model019">; reason?: string };

export type Model019Events = {
  change: Model019Event;
  error: { message: string; code: number };
};

export type Model019Numbers = KeysOfType<Model019Line, number>;
export type FrozenModel019 = DeepReadonly<Model019>;

export function describeModel019Event(event: Model019Event): string {
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

export function totalModel019(item: Model019): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel019(item: Model019): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel019(item), 3), (item.parent ? summarizeModel018(item.parent) : "-"), item.related.map(summarizeModel012).join(",")].join(" | ");
}

export class Model019Service extends Service<Model019, "model019"> {
  readonly events = new EventBus<Model019Events>();
  private readonly parents?: Model018Service;

  constructor(repository = new MemoryRepository<Model019, "model019">()) {
    super(repository);
  }

  validate(item: Model019): string[] {
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

  rename(id: Id<"model019">, to: string): Result<Model019> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model019 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model019">, status: Model019Status): Result<Model019Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model019">, patch: Patch<Pick<Model019, "name" | "tags">>): Model019 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model019Status, Model019[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model019Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model019">[]): Promise<Model019[]> {
    const found: Model019[] = [];
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

  linkedService(): Model018Service {
    return this.parents ?? new Model018Service();
  }
}

export function makeModel019(id: string, name: string): Model019 {
  return {
    id: id as Id<"model019">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 20, unit: "hour" }],
    related: [],
  };
}

export const model019Defaults: FrozenModel019 = makeModel019("default-19", "Default 19");
export const model019Label = summarizeModel019(makeModel019("label", "Label"));
