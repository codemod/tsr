// Generated domain module 199 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model198Service, summarizeModel198 } from "./model198";
import type { Model198 } from "./model198";
import { Model192Service, summarizeModel192 } from "./model192";
import type { Model192 } from "./model192";

export type Model199Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model199Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model199 extends Entity<"model199"> {
  updatedAt?: number;
  name: string;
  status: Model199Status;
  tags: string[];
  lines: Model199Line[];
  owner?: { name: string; email?: string };
  parent?: Model198;
  related: Model192[];
}

export type Model199Event =
  | { kind: "created"; item: Model199 }
  | { kind: "renamed"; id: Id<"model199">; from: string; to: string }
  | { kind: "moved"; id: Id<"model199">; status: Model199Status }
  | { kind: "deleted"; id: Id<"model199">; reason?: string };

export type Model199Events = {
  change: Model199Event;
  error: { message: string; code: number };
};

export type Model199Numbers = KeysOfType<Model199Line, number>;
export type FrozenModel199 = DeepReadonly<Model199>;

export function describeModel199Event(event: Model199Event): string {
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

export function totalModel199(item: Model199): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel199(item: Model199): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel199(item), 3), (item.parent ? summarizeModel198(item.parent) : "-"), item.related.map(summarizeModel192).join(",")].join(" | ");
}

export class Model199Service extends Service<Model199, "model199"> {
  readonly events = new EventBus<Model199Events>();
  private readonly parents?: Model198Service;

  constructor(repository = new MemoryRepository<Model199, "model199">()) {
    super(repository);
  }

  validate(item: Model199): string[] {
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

  rename(id: Id<"model199">, to: string): Result<Model199> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model199 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model199">, status: Model199Status): Result<Model199Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model199">, patch: Patch<Pick<Model199, "name" | "tags">>): Model199 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model199Status, Model199[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model199Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model199">[]): Promise<Model199[]> {
    const found: Model199[] = [];
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

  linkedService(): Model198Service {
    return this.parents ?? new Model198Service();
  }
}

export function makeModel199(id: string, name: string): Model199 {
  return {
    id: id as Id<"model199">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 200, unit: "hour" }],
    related: [],
  };
}

export const model199Defaults: FrozenModel199 = makeModel199("default-199", "Default 199");
export const model199Label = summarizeModel199(makeModel199("label", "Label"));
