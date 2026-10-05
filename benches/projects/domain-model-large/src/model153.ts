// Generated domain module 153 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model152Service, summarizeModel152 } from "./model152";
import type { Model152 } from "./model152";
import { Model146Service, summarizeModel146 } from "./model146";
import type { Model146 } from "./model146";

export type Model153Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model153Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model153 extends Entity<"model153"> {
  updatedAt?: number;
  name: string;
  status: Model153Status;
  tags: string[];
  lines: Model153Line[];
  owner?: { name: string; email?: string };
  parent?: Model152;
  related: Model146[];
}

export type Model153Event =
  | { kind: "created"; item: Model153 }
  | { kind: "renamed"; id: Id<"model153">; from: string; to: string }
  | { kind: "moved"; id: Id<"model153">; status: Model153Status }
  | { kind: "deleted"; id: Id<"model153">; reason?: string };

export type Model153Events = {
  change: Model153Event;
  error: { message: string; code: number };
};

export type Model153Numbers = KeysOfType<Model153Line, number>;
export type FrozenModel153 = DeepReadonly<Model153>;

export function describeModel153Event(event: Model153Event): string {
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

export function totalModel153(item: Model153): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel153(item: Model153): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel153(item), 3), (item.parent ? summarizeModel152(item.parent) : "-"), item.related.map(summarizeModel146).join(",")].join(" | ");
}

export class Model153Service extends Service<Model153, "model153"> {
  readonly events = new EventBus<Model153Events>();
  private readonly parents?: Model152Service;

  constructor(repository = new MemoryRepository<Model153, "model153">()) {
    super(repository);
  }

  validate(item: Model153): string[] {
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

  rename(id: Id<"model153">, to: string): Result<Model153> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model153 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model153">, status: Model153Status): Result<Model153Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model153">, patch: Patch<Pick<Model153, "name" | "tags">>): Model153 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model153Status, Model153[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model153Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model153">[]): Promise<Model153[]> {
    const found: Model153[] = [];
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

  linkedService(): Model152Service {
    return this.parents ?? new Model152Service();
  }
}

export function makeModel153(id: string, name: string): Model153 {
  return {
    id: id as Id<"model153">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 154, unit: "item" }],
    related: [],
  };
}

export const model153Defaults: FrozenModel153 = makeModel153("default-153", "Default 153");
export const model153Label = summarizeModel153(makeModel153("label", "Label"));
