// Generated domain module 14 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model013Service, summarizeModel013 } from "./model013";
import type { Model013 } from "./model013";
import { Model007Service, summarizeModel007 } from "./model007";
import type { Model007 } from "./model007";

export type Model014Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model014Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model014 extends Entity<"model014"> {
  updatedAt?: number;
  name: string;
  status: Model014Status;
  tags: string[];
  lines: Model014Line[];
  owner?: { name: string; email?: string };
  parent?: Model013;
  related: Model007[];
}

export type Model014Event =
  | { kind: "created"; item: Model014 }
  | { kind: "renamed"; id: Id<"model014">; from: string; to: string }
  | { kind: "moved"; id: Id<"model014">; status: Model014Status }
  | { kind: "deleted"; id: Id<"model014">; reason?: string };

export type Model014Events = {
  change: Model014Event;
  error: { message: string; code: number };
};

export type Model014Numbers = KeysOfType<Model014Line, number>;
export type FrozenModel014 = DeepReadonly<Model014>;

export function describeModel014Event(event: Model014Event): string {
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

export function totalModel014(item: Model014): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel014(item: Model014): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel014(item), 3), (item.parent ? summarizeModel013(item.parent) : "-"), item.related.map(summarizeModel007).join(",")].join(" | ");
}

export class Model014Service extends Service<Model014, "model014"> {
  readonly events = new EventBus<Model014Events>();
  private readonly parents?: Model013Service;

  constructor(repository = new MemoryRepository<Model014, "model014">()) {
    super(repository);
  }

  validate(item: Model014): string[] {
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

  rename(id: Id<"model014">, to: string): Result<Model014> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model014 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model014">, status: Model014Status): Result<Model014Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model014">, patch: Patch<Pick<Model014, "name" | "tags">>): Model014 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model014Status, Model014[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model014Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model014">[]): Promise<Model014[]> {
    const found: Model014[] = [];
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

  linkedService(): Model013Service {
    return this.parents ?? new Model013Service();
  }
}

export function makeModel014(id: string, name: string): Model014 {
  return {
    id: id as Id<"model014">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 15, unit: "hour" }],
    related: [],
  };
}

export const model014Defaults: FrozenModel014 = makeModel014("default-14", "Default 14");
export const model014Label = summarizeModel014(makeModel014("label", "Label"));
