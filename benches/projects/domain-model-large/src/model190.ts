// Generated domain module 190 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model189Service, summarizeModel189 } from "./model189";
import type { Model189 } from "./model189";
import { Model183Service, summarizeModel183 } from "./model183";
import type { Model183 } from "./model183";

export type Model190Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model190Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model190 extends Entity<"model190"> {
  updatedAt?: number;
  name: string;
  status: Model190Status;
  tags: string[];
  lines: Model190Line[];
  owner?: { name: string; email?: string };
  parent?: Model189;
  related: Model183[];
}

export type Model190Event =
  | { kind: "created"; item: Model190 }
  | { kind: "renamed"; id: Id<"model190">; from: string; to: string }
  | { kind: "moved"; id: Id<"model190">; status: Model190Status }
  | { kind: "deleted"; id: Id<"model190">; reason?: string };

export type Model190Events = {
  change: Model190Event;
  error: { message: string; code: number };
};

export type Model190Numbers = KeysOfType<Model190Line, number>;
export type FrozenModel190 = DeepReadonly<Model190>;

export function describeModel190Event(event: Model190Event): string {
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

export function totalModel190(item: Model190): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel190(item: Model190): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel190(item), 3), (item.parent ? summarizeModel189(item.parent) : "-"), item.related.map(summarizeModel183).join(",")].join(" | ");
}

export class Model190Service extends Service<Model190, "model190"> {
  readonly events = new EventBus<Model190Events>();
  private readonly parents?: Model189Service;

  constructor(repository = new MemoryRepository<Model190, "model190">()) {
    super(repository);
  }

  validate(item: Model190): string[] {
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

  rename(id: Id<"model190">, to: string): Result<Model190> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model190 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model190">, status: Model190Status): Result<Model190Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model190">, patch: Patch<Pick<Model190, "name" | "tags">>): Model190 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model190Status, Model190[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model190Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model190">[]): Promise<Model190[]> {
    const found: Model190[] = [];
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

  linkedService(): Model189Service {
    return this.parents ?? new Model189Service();
  }
}

export function makeModel190(id: string, name: string): Model190 {
  return {
    id: id as Id<"model190">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 191, unit: "kg" }],
    related: [],
  };
}

export const model190Defaults: FrozenModel190 = makeModel190("default-190", "Default 190");
export const model190Label = summarizeModel190(makeModel190("label", "Label"));
