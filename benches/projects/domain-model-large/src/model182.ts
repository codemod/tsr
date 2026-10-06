// Generated domain module 182 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model181Service, summarizeModel181 } from "./model181";
import type { Model181 } from "./model181";
import { Model175Service, summarizeModel175 } from "./model175";
import type { Model175 } from "./model175";

export type Model182Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model182Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model182 extends Entity<"model182"> {
  updatedAt?: number;
  name: string;
  status: Model182Status;
  tags: string[];
  lines: Model182Line[];
  owner?: { name: string; email?: string };
  parent?: Model181;
  related: Model175[];
}

export type Model182Event =
  | { kind: "created"; item: Model182 }
  | { kind: "renamed"; id: Id<"model182">; from: string; to: string }
  | { kind: "moved"; id: Id<"model182">; status: Model182Status }
  | { kind: "deleted"; id: Id<"model182">; reason?: string };

export type Model182Events = {
  change: Model182Event;
  error: { message: string; code: number };
};

export type Model182Numbers = KeysOfType<Model182Line, number>;
export type FrozenModel182 = DeepReadonly<Model182>;

export function describeModel182Event(event: Model182Event): string {
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

export function totalModel182(item: Model182): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel182(item: Model182): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel182(item), 3), (item.parent ? summarizeModel181(item.parent) : "-"), item.related.map(summarizeModel175).join(",")].join(" | ");
}

export class Model182Service extends Service<Model182, "model182"> {
  readonly events = new EventBus<Model182Events>();
  private readonly parents?: Model181Service;

  constructor(repository = new MemoryRepository<Model182, "model182">()) {
    super(repository);
  }

  validate(item: Model182): string[] {
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

  rename(id: Id<"model182">, to: string): Result<Model182> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model182 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model182">, status: Model182Status): Result<Model182Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model182">, patch: Patch<Pick<Model182, "name" | "tags">>): Model182 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model182Status, Model182[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model182Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model182">[]): Promise<Model182[]> {
    const found: Model182[] = [];
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

  linkedService(): Model181Service {
    return this.parents ?? new Model181Service();
  }
}

export function makeModel182(id: string, name: string): Model182 {
  return {
    id: id as Id<"model182">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 183, unit: "s" }],
    related: [],
  };
}

export const model182Defaults: FrozenModel182 = makeModel182("default-182", "Default 182");
export const model182Label = summarizeModel182(makeModel182("label", "Label"));
