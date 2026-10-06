// Generated domain module 59 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model058Service, summarizeModel058 } from "./model058";
import type { Model058 } from "./model058";
import { Model052Service, summarizeModel052 } from "./model052";
import type { Model052 } from "./model052";

export type Model059Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model059Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model059 extends Entity<"model059"> {
  updatedAt?: number;
  name: string;
  status: Model059Status;
  tags: string[];
  lines: Model059Line[];
  owner?: { name: string; email?: string };
  parent?: Model058;
  related: Model052[];
}

export type Model059Event =
  | { kind: "created"; item: Model059 }
  | { kind: "renamed"; id: Id<"model059">; from: string; to: string }
  | { kind: "moved"; id: Id<"model059">; status: Model059Status }
  | { kind: "deleted"; id: Id<"model059">; reason?: string };

export type Model059Events = {
  change: Model059Event;
  error: { message: string; code: number };
};

export type Model059Numbers = KeysOfType<Model059Line, number>;
export type FrozenModel059 = DeepReadonly<Model059>;

export function describeModel059Event(event: Model059Event): string {
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

export function totalModel059(item: Model059): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel059(item: Model059): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel059(item), 3), (item.parent ? summarizeModel058(item.parent) : "-"), item.related.map(summarizeModel052).join(",")].join(" | ");
}

export class Model059Service extends Service<Model059, "model059"> {
  readonly events = new EventBus<Model059Events>();
  private readonly parents?: Model058Service;

  constructor(repository = new MemoryRepository<Model059, "model059">()) {
    super(repository);
  }

  validate(item: Model059): string[] {
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

  rename(id: Id<"model059">, to: string): Result<Model059> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model059 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model059">, status: Model059Status): Result<Model059Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model059">, patch: Patch<Pick<Model059, "name" | "tags">>): Model059 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model059Status, Model059[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model059Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model059">[]): Promise<Model059[]> {
    const found: Model059[] = [];
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

  linkedService(): Model058Service {
    return this.parents ?? new Model058Service();
  }
}

export function makeModel059(id: string, name: string): Model059 {
  return {
    id: id as Id<"model059">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 60, unit: "hour" }],
    related: [],
  };
}

export const model059Defaults: FrozenModel059 = makeModel059("default-59", "Default 59");
export const model059Label = summarizeModel059(makeModel059("label", "Label"));
