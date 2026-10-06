// Generated domain module 164 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model163Service, summarizeModel163 } from "./model163";
import type { Model163 } from "./model163";
import { Model157Service, summarizeModel157 } from "./model157";
import type { Model157 } from "./model157";

export type Model164Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model164Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model164 extends Entity<"model164"> {
  updatedAt?: number;
  name: string;
  status: Model164Status;
  tags: string[];
  lines: Model164Line[];
  owner?: { name: string; email?: string };
  parent?: Model163;
  related: Model157[];
}

export type Model164Event =
  | { kind: "created"; item: Model164 }
  | { kind: "renamed"; id: Id<"model164">; from: string; to: string }
  | { kind: "moved"; id: Id<"model164">; status: Model164Status }
  | { kind: "deleted"; id: Id<"model164">; reason?: string };

export type Model164Events = {
  change: Model164Event;
  error: { message: string; code: number };
};

export type Model164Numbers = KeysOfType<Model164Line, number>;
export type FrozenModel164 = DeepReadonly<Model164>;

export function describeModel164Event(event: Model164Event): string {
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

export function totalModel164(item: Model164): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel164(item: Model164): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel164(item), 3), (item.parent ? summarizeModel163(item.parent) : "-"), item.related.map(summarizeModel157).join(",")].join(" | ");
}

export class Model164Service extends Service<Model164, "model164"> {
  readonly events = new EventBus<Model164Events>();
  private readonly parents?: Model163Service;

  constructor(repository = new MemoryRepository<Model164, "model164">()) {
    super(repository);
  }

  validate(item: Model164): string[] {
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

  rename(id: Id<"model164">, to: string): Result<Model164> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model164 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model164">, status: Model164Status): Result<Model164Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model164">, patch: Patch<Pick<Model164, "name" | "tags">>): Model164 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model164Status, Model164[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model164Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model164">[]): Promise<Model164[]> {
    const found: Model164[] = [];
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

  linkedService(): Model163Service {
    return this.parents ?? new Model163Service();
  }
}

export function makeModel164(id: string, name: string): Model164 {
  return {
    id: id as Id<"model164">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 165, unit: "hour" }],
    related: [],
  };
}

export const model164Defaults: FrozenModel164 = makeModel164("default-164", "Default 164");
export const model164Label = summarizeModel164(makeModel164("label", "Label"));
