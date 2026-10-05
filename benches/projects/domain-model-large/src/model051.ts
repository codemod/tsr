// Generated domain module 51 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model050Service, summarizeModel050 } from "./model050";
import type { Model050 } from "./model050";
import { Model044Service, summarizeModel044 } from "./model044";
import type { Model044 } from "./model044";

export type Model051Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model051Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model051 extends Entity<"model051"> {
  updatedAt?: number;
  name: string;
  status: Model051Status;
  tags: string[];
  lines: Model051Line[];
  owner?: { name: string; email?: string };
  parent?: Model050;
  related: Model044[];
}

export type Model051Event =
  | { kind: "created"; item: Model051 }
  | { kind: "renamed"; id: Id<"model051">; from: string; to: string }
  | { kind: "moved"; id: Id<"model051">; status: Model051Status }
  | { kind: "deleted"; id: Id<"model051">; reason?: string };

export type Model051Events = {
  change: Model051Event;
  error: { message: string; code: number };
};

export type Model051Numbers = KeysOfType<Model051Line, number>;
export type FrozenModel051 = DeepReadonly<Model051>;

export function describeModel051Event(event: Model051Event): string {
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

export function totalModel051(item: Model051): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel051(item: Model051): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel051(item), 3), (item.parent ? summarizeModel050(item.parent) : "-"), item.related.map(summarizeModel044).join(",")].join(" | ");
}

export class Model051Service extends Service<Model051, "model051"> {
  readonly events = new EventBus<Model051Events>();
  private readonly parents?: Model050Service;

  constructor(repository = new MemoryRepository<Model051, "model051">()) {
    super(repository);
  }

  validate(item: Model051): string[] {
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

  rename(id: Id<"model051">, to: string): Result<Model051> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model051 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model051">, status: Model051Status): Result<Model051Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model051">, patch: Patch<Pick<Model051, "name" | "tags">>): Model051 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model051Status, Model051[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model051Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model051">[]): Promise<Model051[]> {
    const found: Model051[] = [];
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

  linkedService(): Model050Service {
    return this.parents ?? new Model050Service();
  }
}

export function makeModel051(id: string, name: string): Model051 {
  return {
    id: id as Id<"model051">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 52, unit: "m" }],
    related: [],
  };
}

export const model051Defaults: FrozenModel051 = makeModel051("default-51", "Default 51");
export const model051Label = summarizeModel051(makeModel051("label", "Label"));
