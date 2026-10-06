// Generated domain module 130 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model129Service, summarizeModel129 } from "./model129";
import type { Model129 } from "./model129";
import { Model123Service, summarizeModel123 } from "./model123";
import type { Model123 } from "./model123";

export type Model130Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model130Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model130 extends Entity<"model130"> {
  updatedAt?: number;
  name: string;
  status: Model130Status;
  tags: string[];
  lines: Model130Line[];
  owner?: { name: string; email?: string };
  parent?: Model129;
  related: Model123[];
}

export type Model130Event =
  | { kind: "created"; item: Model130 }
  | { kind: "renamed"; id: Id<"model130">; from: string; to: string }
  | { kind: "moved"; id: Id<"model130">; status: Model130Status }
  | { kind: "deleted"; id: Id<"model130">; reason?: string };

export type Model130Events = {
  change: Model130Event;
  error: { message: string; code: number };
};

export type Model130Numbers = KeysOfType<Model130Line, number>;
export type FrozenModel130 = DeepReadonly<Model130>;

export function describeModel130Event(event: Model130Event): string {
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

export function totalModel130(item: Model130): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel130(item: Model130): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel130(item), 3), (item.parent ? summarizeModel129(item.parent) : "-"), item.related.map(summarizeModel123).join(",")].join(" | ");
}

export class Model130Service extends Service<Model130, "model130"> {
  readonly events = new EventBus<Model130Events>();
  private readonly parents?: Model129Service;

  constructor(repository = new MemoryRepository<Model130, "model130">()) {
    super(repository);
  }

  validate(item: Model130): string[] {
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

  rename(id: Id<"model130">, to: string): Result<Model130> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model130 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model130">, status: Model130Status): Result<Model130Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model130">, patch: Patch<Pick<Model130, "name" | "tags">>): Model130 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model130Status, Model130[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model130Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model130">[]): Promise<Model130[]> {
    const found: Model130[] = [];
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

  linkedService(): Model129Service {
    return this.parents ?? new Model129Service();
  }
}

export function makeModel130(id: string, name: string): Model130 {
  return {
    id: id as Id<"model130">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 131, unit: "kg" }],
    related: [],
  };
}

export const model130Defaults: FrozenModel130 = makeModel130("default-130", "Default 130");
export const model130Label = summarizeModel130(makeModel130("label", "Label"));
