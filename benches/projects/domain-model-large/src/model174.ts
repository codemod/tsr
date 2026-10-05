// Generated domain module 174 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model173Service, summarizeModel173 } from "./model173";
import type { Model173 } from "./model173";
import { Model167Service, summarizeModel167 } from "./model167";
import type { Model167 } from "./model167";

export type Model174Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model174Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model174 extends Entity<"model174"> {
  updatedAt?: number;
  name: string;
  status: Model174Status;
  tags: string[];
  lines: Model174Line[];
  owner?: { name: string; email?: string };
  parent?: Model173;
  related: Model167[];
}

export type Model174Event =
  | { kind: "created"; item: Model174 }
  | { kind: "renamed"; id: Id<"model174">; from: string; to: string }
  | { kind: "moved"; id: Id<"model174">; status: Model174Status }
  | { kind: "deleted"; id: Id<"model174">; reason?: string };

export type Model174Events = {
  change: Model174Event;
  error: { message: string; code: number };
};

export type Model174Numbers = KeysOfType<Model174Line, number>;
export type FrozenModel174 = DeepReadonly<Model174>;

export function describeModel174Event(event: Model174Event): string {
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

export function totalModel174(item: Model174): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel174(item: Model174): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel174(item), 3), (item.parent ? summarizeModel173(item.parent) : "-"), item.related.map(summarizeModel167).join(",")].join(" | ");
}

export class Model174Service extends Service<Model174, "model174"> {
  readonly events = new EventBus<Model174Events>();
  private readonly parents?: Model173Service;

  constructor(repository = new MemoryRepository<Model174, "model174">()) {
    super(repository);
  }

  validate(item: Model174): string[] {
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

  rename(id: Id<"model174">, to: string): Result<Model174> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model174 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model174">, status: Model174Status): Result<Model174Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model174">, patch: Patch<Pick<Model174, "name" | "tags">>): Model174 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model174Status, Model174[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model174Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model174">[]): Promise<Model174[]> {
    const found: Model174[] = [];
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

  linkedService(): Model173Service {
    return this.parents ?? new Model173Service();
  }
}

export function makeModel174(id: string, name: string): Model174 {
  return {
    id: id as Id<"model174">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 175, unit: "hour" }],
    related: [],
  };
}

export const model174Defaults: FrozenModel174 = makeModel174("default-174", "Default 174");
export const model174Label = summarizeModel174(makeModel174("label", "Label"));
