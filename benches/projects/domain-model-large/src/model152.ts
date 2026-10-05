// Generated domain module 152 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model151Service, summarizeModel151 } from "./model151";
import type { Model151 } from "./model151";
import { Model145Service, summarizeModel145 } from "./model145";
import type { Model145 } from "./model145";

export type Model152Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model152Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model152 extends Entity<"model152"> {
  updatedAt?: number;
  name: string;
  status: Model152Status;
  tags: string[];
  lines: Model152Line[];
  owner?: { name: string; email?: string };
  parent?: Model151;
  related: Model145[];
}

export type Model152Event =
  | { kind: "created"; item: Model152 }
  | { kind: "renamed"; id: Id<"model152">; from: string; to: string }
  | { kind: "moved"; id: Id<"model152">; status: Model152Status }
  | { kind: "deleted"; id: Id<"model152">; reason?: string };

export type Model152Events = {
  change: Model152Event;
  error: { message: string; code: number };
};

export type Model152Numbers = KeysOfType<Model152Line, number>;
export type FrozenModel152 = DeepReadonly<Model152>;

export function describeModel152Event(event: Model152Event): string {
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

export function totalModel152(item: Model152): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel152(item: Model152): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel152(item), 3), (item.parent ? summarizeModel151(item.parent) : "-"), item.related.map(summarizeModel145).join(",")].join(" | ");
}

export class Model152Service extends Service<Model152, "model152"> {
  readonly events = new EventBus<Model152Events>();
  private readonly parents?: Model151Service;

  constructor(repository = new MemoryRepository<Model152, "model152">()) {
    super(repository);
  }

  validate(item: Model152): string[] {
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

  rename(id: Id<"model152">, to: string): Result<Model152> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model152 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model152">, status: Model152Status): Result<Model152Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model152">, patch: Patch<Pick<Model152, "name" | "tags">>): Model152 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model152Status, Model152[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model152Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model152">[]): Promise<Model152[]> {
    const found: Model152[] = [];
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

  linkedService(): Model151Service {
    return this.parents ?? new Model151Service();
  }
}

export function makeModel152(id: string, name: string): Model152 {
  return {
    id: id as Id<"model152">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 153, unit: "s" }],
    related: [],
  };
}

export const model152Defaults: FrozenModel152 = makeModel152("default-152", "Default 152");
export const model152Label = summarizeModel152(makeModel152("label", "Label"));
