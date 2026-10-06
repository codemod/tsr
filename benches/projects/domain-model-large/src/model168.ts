// Generated domain module 168 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model167Service, summarizeModel167 } from "./model167";
import type { Model167 } from "./model167";
import { Model161Service, summarizeModel161 } from "./model161";
import type { Model161 } from "./model161";

export type Model168Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model168Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model168 extends Entity<"model168"> {
  updatedAt?: number;
  name: string;
  status: Model168Status;
  tags: string[];
  lines: Model168Line[];
  owner?: { name: string; email?: string };
  parent?: Model167;
  related: Model161[];
}

export type Model168Event =
  | { kind: "created"; item: Model168 }
  | { kind: "renamed"; id: Id<"model168">; from: string; to: string }
  | { kind: "moved"; id: Id<"model168">; status: Model168Status }
  | { kind: "deleted"; id: Id<"model168">; reason?: string };

export type Model168Events = {
  change: Model168Event;
  error: { message: string; code: number };
};

export type Model168Numbers = KeysOfType<Model168Line, number>;
export type FrozenModel168 = DeepReadonly<Model168>;

export function describeModel168Event(event: Model168Event): string {
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

export function totalModel168(item: Model168): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel168(item: Model168): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel168(item), 3), (item.parent ? summarizeModel167(item.parent) : "-"), item.related.map(summarizeModel161).join(",")].join(" | ");
}

export class Model168Service extends Service<Model168, "model168"> {
  readonly events = new EventBus<Model168Events>();
  private readonly parents?: Model167Service;

  constructor(repository = new MemoryRepository<Model168, "model168">()) {
    super(repository);
  }

  validate(item: Model168): string[] {
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

  rename(id: Id<"model168">, to: string): Result<Model168> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model168 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model168">, status: Model168Status): Result<Model168Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model168">, patch: Patch<Pick<Model168, "name" | "tags">>): Model168 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model168Status, Model168[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model168Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model168">[]): Promise<Model168[]> {
    const found: Model168[] = [];
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

  linkedService(): Model167Service {
    return this.parents ?? new Model167Service();
  }
}

export function makeModel168(id: string, name: string): Model168 {
  return {
    id: id as Id<"model168">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 169, unit: "item" }],
    related: [],
  };
}

export const model168Defaults: FrozenModel168 = makeModel168("default-168", "Default 168");
export const model168Label = summarizeModel168(makeModel168("label", "Label"));
