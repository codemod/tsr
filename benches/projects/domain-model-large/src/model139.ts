// Generated domain module 139 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model138Service, summarizeModel138 } from "./model138";
import type { Model138 } from "./model138";
import { Model132Service, summarizeModel132 } from "./model132";
import type { Model132 } from "./model132";

export type Model139Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model139Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model139 extends Entity<"model139"> {
  updatedAt?: number;
  name: string;
  status: Model139Status;
  tags: string[];
  lines: Model139Line[];
  owner?: { name: string; email?: string };
  parent?: Model138;
  related: Model132[];
}

export type Model139Event =
  | { kind: "created"; item: Model139 }
  | { kind: "renamed"; id: Id<"model139">; from: string; to: string }
  | { kind: "moved"; id: Id<"model139">; status: Model139Status }
  | { kind: "deleted"; id: Id<"model139">; reason?: string };

export type Model139Events = {
  change: Model139Event;
  error: { message: string; code: number };
};

export type Model139Numbers = KeysOfType<Model139Line, number>;
export type FrozenModel139 = DeepReadonly<Model139>;

export function describeModel139Event(event: Model139Event): string {
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

export function totalModel139(item: Model139): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel139(item: Model139): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel139(item), 3), (item.parent ? summarizeModel138(item.parent) : "-"), item.related.map(summarizeModel132).join(",")].join(" | ");
}

export class Model139Service extends Service<Model139, "model139"> {
  readonly events = new EventBus<Model139Events>();
  private readonly parents?: Model138Service;

  constructor(repository = new MemoryRepository<Model139, "model139">()) {
    super(repository);
  }

  validate(item: Model139): string[] {
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

  rename(id: Id<"model139">, to: string): Result<Model139> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model139 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model139">, status: Model139Status): Result<Model139Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model139">, patch: Patch<Pick<Model139, "name" | "tags">>): Model139 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model139Status, Model139[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model139Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model139">[]): Promise<Model139[]> {
    const found: Model139[] = [];
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

  linkedService(): Model138Service {
    return this.parents ?? new Model138Service();
  }
}

export function makeModel139(id: string, name: string): Model139 {
  return {
    id: id as Id<"model139">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 140, unit: "hour" }],
    related: [],
  };
}

export const model139Defaults: FrozenModel139 = makeModel139("default-139", "Default 139");
export const model139Label = summarizeModel139(makeModel139("label", "Label"));
