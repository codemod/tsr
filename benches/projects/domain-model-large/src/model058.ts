// Generated domain module 58 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model057Service, summarizeModel057 } from "./model057";
import type { Model057 } from "./model057";
import { Model051Service, summarizeModel051 } from "./model051";
import type { Model051 } from "./model051";

export type Model058Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model058Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model058 extends Entity<"model058"> {
  updatedAt?: number;
  name: string;
  status: Model058Status;
  tags: string[];
  lines: Model058Line[];
  owner?: { name: string; email?: string };
  parent?: Model057;
  related: Model051[];
}

export type Model058Event =
  | { kind: "created"; item: Model058 }
  | { kind: "renamed"; id: Id<"model058">; from: string; to: string }
  | { kind: "moved"; id: Id<"model058">; status: Model058Status }
  | { kind: "deleted"; id: Id<"model058">; reason?: string };

export type Model058Events = {
  change: Model058Event;
  error: { message: string; code: number };
};

export type Model058Numbers = KeysOfType<Model058Line, number>;
export type FrozenModel058 = DeepReadonly<Model058>;

export function describeModel058Event(event: Model058Event): string {
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

export function totalModel058(item: Model058): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel058(item: Model058): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel058(item), 3), (item.parent ? summarizeModel057(item.parent) : "-"), item.related.map(summarizeModel051).join(",")].join(" | ");
}

export class Model058Service extends Service<Model058, "model058"> {
  readonly events = new EventBus<Model058Events>();
  private readonly parents?: Model057Service;

  constructor(repository = new MemoryRepository<Model058, "model058">()) {
    super(repository);
  }

  validate(item: Model058): string[] {
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

  rename(id: Id<"model058">, to: string): Result<Model058> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model058 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model058">, status: Model058Status): Result<Model058Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model058">, patch: Patch<Pick<Model058, "name" | "tags">>): Model058 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model058Status, Model058[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model058Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model058">[]): Promise<Model058[]> {
    const found: Model058[] = [];
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

  linkedService(): Model057Service {
    return this.parents ?? new Model057Service();
  }
}

export function makeModel058(id: string, name: string): Model058 {
  return {
    id: id as Id<"model058">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 59, unit: "item" }],
    related: [],
  };
}

export const model058Defaults: FrozenModel058 = makeModel058("default-58", "Default 58");
export const model058Label = summarizeModel058(makeModel058("label", "Label"));
