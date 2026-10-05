// Generated domain module 198 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model197Service, summarizeModel197 } from "./model197";
import type { Model197 } from "./model197";
import { Model191Service, summarizeModel191 } from "./model191";
import type { Model191 } from "./model191";

export type Model198Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model198Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model198 extends Entity<"model198"> {
  updatedAt?: number;
  name: string;
  status: Model198Status;
  tags: string[];
  lines: Model198Line[];
  owner?: { name: string; email?: string };
  parent?: Model197;
  related: Model191[];
}

export type Model198Event =
  | { kind: "created"; item: Model198 }
  | { kind: "renamed"; id: Id<"model198">; from: string; to: string }
  | { kind: "moved"; id: Id<"model198">; status: Model198Status }
  | { kind: "deleted"; id: Id<"model198">; reason?: string };

export type Model198Events = {
  change: Model198Event;
  error: { message: string; code: number };
};

export type Model198Numbers = KeysOfType<Model198Line, number>;
export type FrozenModel198 = DeepReadonly<Model198>;

export function describeModel198Event(event: Model198Event): string {
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

export function totalModel198(item: Model198): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel198(item: Model198): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel198(item), 3), (item.parent ? summarizeModel197(item.parent) : "-"), item.related.map(summarizeModel191).join(",")].join(" | ");
}

export class Model198Service extends Service<Model198, "model198"> {
  readonly events = new EventBus<Model198Events>();
  private readonly parents?: Model197Service;

  constructor(repository = new MemoryRepository<Model198, "model198">()) {
    super(repository);
  }

  validate(item: Model198): string[] {
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

  rename(id: Id<"model198">, to: string): Result<Model198> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model198 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model198">, status: Model198Status): Result<Model198Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model198">, patch: Patch<Pick<Model198, "name" | "tags">>): Model198 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model198Status, Model198[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model198Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model198">[]): Promise<Model198[]> {
    const found: Model198[] = [];
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

  linkedService(): Model197Service {
    return this.parents ?? new Model197Service();
  }
}

export function makeModel198(id: string, name: string): Model198 {
  return {
    id: id as Id<"model198">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 199, unit: "item" }],
    related: [],
  };
}

export const model198Defaults: FrozenModel198 = makeModel198("default-198", "Default 198");
export const model198Label = summarizeModel198(makeModel198("label", "Label"));
