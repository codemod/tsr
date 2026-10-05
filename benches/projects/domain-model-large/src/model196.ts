// Generated domain module 196 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model195Service, summarizeModel195 } from "./model195";
import type { Model195 } from "./model195";
import { Model189Service, summarizeModel189 } from "./model189";
import type { Model189 } from "./model189";

export type Model196Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model196Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model196 extends Entity<"model196"> {
  updatedAt?: number;
  name: string;
  status: Model196Status;
  tags: string[];
  lines: Model196Line[];
  owner?: { name: string; email?: string };
  parent?: Model195;
  related: Model189[];
}

export type Model196Event =
  | { kind: "created"; item: Model196 }
  | { kind: "renamed"; id: Id<"model196">; from: string; to: string }
  | { kind: "moved"; id: Id<"model196">; status: Model196Status }
  | { kind: "deleted"; id: Id<"model196">; reason?: string };

export type Model196Events = {
  change: Model196Event;
  error: { message: string; code: number };
};

export type Model196Numbers = KeysOfType<Model196Line, number>;
export type FrozenModel196 = DeepReadonly<Model196>;

export function describeModel196Event(event: Model196Event): string {
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

export function totalModel196(item: Model196): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel196(item: Model196): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel196(item), 3), (item.parent ? summarizeModel195(item.parent) : "-"), item.related.map(summarizeModel189).join(",")].join(" | ");
}

export class Model196Service extends Service<Model196, "model196"> {
  readonly events = new EventBus<Model196Events>();
  private readonly parents?: Model195Service;

  constructor(repository = new MemoryRepository<Model196, "model196">()) {
    super(repository);
  }

  validate(item: Model196): string[] {
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

  rename(id: Id<"model196">, to: string): Result<Model196> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model196 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model196">, status: Model196Status): Result<Model196Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model196">, patch: Patch<Pick<Model196, "name" | "tags">>): Model196 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model196Status, Model196[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model196Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model196">[]): Promise<Model196[]> {
    const found: Model196[] = [];
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

  linkedService(): Model195Service {
    return this.parents ?? new Model195Service();
  }
}

export function makeModel196(id: string, name: string): Model196 {
  return {
    id: id as Id<"model196">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 197, unit: "m" }],
    related: [],
  };
}

export const model196Defaults: FrozenModel196 = makeModel196("default-196", "Default 196");
export const model196Label = summarizeModel196(makeModel196("label", "Label"));
