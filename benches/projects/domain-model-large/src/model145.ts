// Generated domain module 145 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model144Service, summarizeModel144 } from "./model144";
import type { Model144 } from "./model144";
import { Model138Service, summarizeModel138 } from "./model138";
import type { Model138 } from "./model138";

export type Model145Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model145Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model145 extends Entity<"model145"> {
  updatedAt?: number;
  name: string;
  status: Model145Status;
  tags: string[];
  lines: Model145Line[];
  owner?: { name: string; email?: string };
  parent?: Model144;
  related: Model138[];
}

export type Model145Event =
  | { kind: "created"; item: Model145 }
  | { kind: "renamed"; id: Id<"model145">; from: string; to: string }
  | { kind: "moved"; id: Id<"model145">; status: Model145Status }
  | { kind: "deleted"; id: Id<"model145">; reason?: string };

export type Model145Events = {
  change: Model145Event;
  error: { message: string; code: number };
};

export type Model145Numbers = KeysOfType<Model145Line, number>;
export type FrozenModel145 = DeepReadonly<Model145>;

export function describeModel145Event(event: Model145Event): string {
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

export function totalModel145(item: Model145): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel145(item: Model145): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel145(item), 3), (item.parent ? summarizeModel144(item.parent) : "-"), item.related.map(summarizeModel138).join(",")].join(" | ");
}

export class Model145Service extends Service<Model145, "model145"> {
  readonly events = new EventBus<Model145Events>();
  private readonly parents?: Model144Service;

  constructor(repository = new MemoryRepository<Model145, "model145">()) {
    super(repository);
  }

  validate(item: Model145): string[] {
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

  rename(id: Id<"model145">, to: string): Result<Model145> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model145 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model145">, status: Model145Status): Result<Model145Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model145">, patch: Patch<Pick<Model145, "name" | "tags">>): Model145 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model145Status, Model145[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model145Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model145">[]): Promise<Model145[]> {
    const found: Model145[] = [];
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

  linkedService(): Model144Service {
    return this.parents ?? new Model144Service();
  }
}

export function makeModel145(id: string, name: string): Model145 {
  return {
    id: id as Id<"model145">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 146, unit: "kg" }],
    related: [],
  };
}

export const model145Defaults: FrozenModel145 = makeModel145("default-145", "Default 145");
export const model145Label = summarizeModel145(makeModel145("label", "Label"));
