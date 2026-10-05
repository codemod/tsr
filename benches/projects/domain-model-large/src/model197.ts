// Generated domain module 197 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model196Service, summarizeModel196 } from "./model196";
import type { Model196 } from "./model196";
import { Model190Service, summarizeModel190 } from "./model190";
import type { Model190 } from "./model190";

export type Model197Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model197Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model197 extends Entity<"model197"> {
  updatedAt?: number;
  name: string;
  status: Model197Status;
  tags: string[];
  lines: Model197Line[];
  owner?: { name: string; email?: string };
  parent?: Model196;
  related: Model190[];
}

export type Model197Event =
  | { kind: "created"; item: Model197 }
  | { kind: "renamed"; id: Id<"model197">; from: string; to: string }
  | { kind: "moved"; id: Id<"model197">; status: Model197Status }
  | { kind: "deleted"; id: Id<"model197">; reason?: string };

export type Model197Events = {
  change: Model197Event;
  error: { message: string; code: number };
};

export type Model197Numbers = KeysOfType<Model197Line, number>;
export type FrozenModel197 = DeepReadonly<Model197>;

export function describeModel197Event(event: Model197Event): string {
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

export function totalModel197(item: Model197): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel197(item: Model197): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel197(item), 3), (item.parent ? summarizeModel196(item.parent) : "-"), item.related.map(summarizeModel190).join(",")].join(" | ");
}

export class Model197Service extends Service<Model197, "model197"> {
  readonly events = new EventBus<Model197Events>();
  private readonly parents?: Model196Service;

  constructor(repository = new MemoryRepository<Model197, "model197">()) {
    super(repository);
  }

  validate(item: Model197): string[] {
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

  rename(id: Id<"model197">, to: string): Result<Model197> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model197 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model197">, status: Model197Status): Result<Model197Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model197">, patch: Patch<Pick<Model197, "name" | "tags">>): Model197 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model197Status, Model197[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model197Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model197">[]): Promise<Model197[]> {
    const found: Model197[] = [];
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

  linkedService(): Model196Service {
    return this.parents ?? new Model196Service();
  }
}

export function makeModel197(id: string, name: string): Model197 {
  return {
    id: id as Id<"model197">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 198, unit: "s" }],
    related: [],
  };
}

export const model197Defaults: FrozenModel197 = makeModel197("default-197", "Default 197");
export const model197Label = summarizeModel197(makeModel197("label", "Label"));
