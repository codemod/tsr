// Generated domain module 118 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model117Service, summarizeModel117 } from "./model117";
import type { Model117 } from "./model117";
import { Model111Service, summarizeModel111 } from "./model111";
import type { Model111 } from "./model111";

export type Model118Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model118Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model118 extends Entity<"model118"> {
  updatedAt?: number;
  name: string;
  status: Model118Status;
  tags: string[];
  lines: Model118Line[];
  owner?: { name: string; email?: string };
  parent?: Model117;
  related: Model111[];
}

export type Model118Event =
  | { kind: "created"; item: Model118 }
  | { kind: "renamed"; id: Id<"model118">; from: string; to: string }
  | { kind: "moved"; id: Id<"model118">; status: Model118Status }
  | { kind: "deleted"; id: Id<"model118">; reason?: string };

export type Model118Events = {
  change: Model118Event;
  error: { message: string; code: number };
};

export type Model118Numbers = KeysOfType<Model118Line, number>;
export type FrozenModel118 = DeepReadonly<Model118>;

export function describeModel118Event(event: Model118Event): string {
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

export function totalModel118(item: Model118): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel118(item: Model118): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel118(item), 3), (item.parent ? summarizeModel117(item.parent) : "-"), item.related.map(summarizeModel111).join(",")].join(" | ");
}

export class Model118Service extends Service<Model118, "model118"> {
  readonly events = new EventBus<Model118Events>();
  private readonly parents?: Model117Service;

  constructor(repository = new MemoryRepository<Model118, "model118">()) {
    super(repository);
  }

  validate(item: Model118): string[] {
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

  rename(id: Id<"model118">, to: string): Result<Model118> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model118 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model118">, status: Model118Status): Result<Model118Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model118">, patch: Patch<Pick<Model118, "name" | "tags">>): Model118 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model118Status, Model118[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model118Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model118">[]): Promise<Model118[]> {
    const found: Model118[] = [];
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

  linkedService(): Model117Service {
    return this.parents ?? new Model117Service();
  }
}

export function makeModel118(id: string, name: string): Model118 {
  return {
    id: id as Id<"model118">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 119, unit: "item" }],
    related: [],
  };
}

export const model118Defaults: FrozenModel118 = makeModel118("default-118", "Default 118");
export const model118Label = summarizeModel118(makeModel118("label", "Label"));
