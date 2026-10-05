// Generated domain module 184 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model183Service, summarizeModel183 } from "./model183";
import type { Model183 } from "./model183";
import { Model177Service, summarizeModel177 } from "./model177";
import type { Model177 } from "./model177";

export type Model184Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model184Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model184 extends Entity<"model184"> {
  updatedAt?: number;
  name: string;
  status: Model184Status;
  tags: string[];
  lines: Model184Line[];
  owner?: { name: string; email?: string };
  parent?: Model183;
  related: Model177[];
}

export type Model184Event =
  | { kind: "created"; item: Model184 }
  | { kind: "renamed"; id: Id<"model184">; from: string; to: string }
  | { kind: "moved"; id: Id<"model184">; status: Model184Status }
  | { kind: "deleted"; id: Id<"model184">; reason?: string };

export type Model184Events = {
  change: Model184Event;
  error: { message: string; code: number };
};

export type Model184Numbers = KeysOfType<Model184Line, number>;
export type FrozenModel184 = DeepReadonly<Model184>;

export function describeModel184Event(event: Model184Event): string {
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

export function totalModel184(item: Model184): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel184(item: Model184): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel184(item), 3), (item.parent ? summarizeModel183(item.parent) : "-"), item.related.map(summarizeModel177).join(",")].join(" | ");
}

export class Model184Service extends Service<Model184, "model184"> {
  readonly events = new EventBus<Model184Events>();
  private readonly parents?: Model183Service;

  constructor(repository = new MemoryRepository<Model184, "model184">()) {
    super(repository);
  }

  validate(item: Model184): string[] {
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

  rename(id: Id<"model184">, to: string): Result<Model184> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model184 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model184">, status: Model184Status): Result<Model184Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model184">, patch: Patch<Pick<Model184, "name" | "tags">>): Model184 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model184Status, Model184[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model184Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model184">[]): Promise<Model184[]> {
    const found: Model184[] = [];
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

  linkedService(): Model183Service {
    return this.parents ?? new Model183Service();
  }
}

export function makeModel184(id: string, name: string): Model184 {
  return {
    id: id as Id<"model184">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 185, unit: "hour" }],
    related: [],
  };
}

export const model184Defaults: FrozenModel184 = makeModel184("default-184", "Default 184");
export const model184Label = summarizeModel184(makeModel184("label", "Label"));
