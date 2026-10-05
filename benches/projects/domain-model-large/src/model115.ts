// Generated domain module 115 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model114Service, summarizeModel114 } from "./model114";
import type { Model114 } from "./model114";
import { Model108Service, summarizeModel108 } from "./model108";
import type { Model108 } from "./model108";

export type Model115Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model115Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model115 extends Entity<"model115"> {
  updatedAt?: number;
  name: string;
  status: Model115Status;
  tags: string[];
  lines: Model115Line[];
  owner?: { name: string; email?: string };
  parent?: Model114;
  related: Model108[];
}

export type Model115Event =
  | { kind: "created"; item: Model115 }
  | { kind: "renamed"; id: Id<"model115">; from: string; to: string }
  | { kind: "moved"; id: Id<"model115">; status: Model115Status }
  | { kind: "deleted"; id: Id<"model115">; reason?: string };

export type Model115Events = {
  change: Model115Event;
  error: { message: string; code: number };
};

export type Model115Numbers = KeysOfType<Model115Line, number>;
export type FrozenModel115 = DeepReadonly<Model115>;

export function describeModel115Event(event: Model115Event): string {
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

export function totalModel115(item: Model115): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel115(item: Model115): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel115(item), 3), (item.parent ? summarizeModel114(item.parent) : "-"), item.related.map(summarizeModel108).join(",")].join(" | ");
}

export class Model115Service extends Service<Model115, "model115"> {
  readonly events = new EventBus<Model115Events>();
  private readonly parents?: Model114Service;

  constructor(repository = new MemoryRepository<Model115, "model115">()) {
    super(repository);
  }

  validate(item: Model115): string[] {
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

  rename(id: Id<"model115">, to: string): Result<Model115> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model115 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model115">, status: Model115Status): Result<Model115Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model115">, patch: Patch<Pick<Model115, "name" | "tags">>): Model115 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model115Status, Model115[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model115Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model115">[]): Promise<Model115[]> {
    const found: Model115[] = [];
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

  linkedService(): Model114Service {
    return this.parents ?? new Model114Service();
  }
}

export function makeModel115(id: string, name: string): Model115 {
  return {
    id: id as Id<"model115">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 116, unit: "kg" }],
    related: [],
  };
}

export const model115Defaults: FrozenModel115 = makeModel115("default-115", "Default 115");
export const model115Label = summarizeModel115(makeModel115("label", "Label"));
