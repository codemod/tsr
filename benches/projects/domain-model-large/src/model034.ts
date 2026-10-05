// Generated domain module 34 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model033Service, summarizeModel033 } from "./model033";
import type { Model033 } from "./model033";
import { Model027Service, summarizeModel027 } from "./model027";
import type { Model027 } from "./model027";

export type Model034Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model034Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model034 extends Entity<"model034"> {
  updatedAt?: number;
  name: string;
  status: Model034Status;
  tags: string[];
  lines: Model034Line[];
  owner?: { name: string; email?: string };
  parent?: Model033;
  related: Model027[];
}

export type Model034Event =
  | { kind: "created"; item: Model034 }
  | { kind: "renamed"; id: Id<"model034">; from: string; to: string }
  | { kind: "moved"; id: Id<"model034">; status: Model034Status }
  | { kind: "deleted"; id: Id<"model034">; reason?: string };

export type Model034Events = {
  change: Model034Event;
  error: { message: string; code: number };
};

export type Model034Numbers = KeysOfType<Model034Line, number>;
export type FrozenModel034 = DeepReadonly<Model034>;

export function describeModel034Event(event: Model034Event): string {
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

export function totalModel034(item: Model034): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel034(item: Model034): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel034(item), 3), (item.parent ? summarizeModel033(item.parent) : "-"), item.related.map(summarizeModel027).join(",")].join(" | ");
}

export class Model034Service extends Service<Model034, "model034"> {
  readonly events = new EventBus<Model034Events>();
  private readonly parents?: Model033Service;

  constructor(repository = new MemoryRepository<Model034, "model034">()) {
    super(repository);
  }

  validate(item: Model034): string[] {
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

  rename(id: Id<"model034">, to: string): Result<Model034> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model034 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model034">, status: Model034Status): Result<Model034Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model034">, patch: Patch<Pick<Model034, "name" | "tags">>): Model034 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model034Status, Model034[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model034Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model034">[]): Promise<Model034[]> {
    const found: Model034[] = [];
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

  linkedService(): Model033Service {
    return this.parents ?? new Model033Service();
  }
}

export function makeModel034(id: string, name: string): Model034 {
  return {
    id: id as Id<"model034">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 35, unit: "hour" }],
    related: [],
  };
}

export const model034Defaults: FrozenModel034 = makeModel034("default-34", "Default 34");
export const model034Label = summarizeModel034(makeModel034("label", "Label"));
