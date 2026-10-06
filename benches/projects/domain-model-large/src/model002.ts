// Generated domain module 2 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model001Service, summarizeModel001 } from "./model001";
import type { Model001 } from "./model001";

export type Model002Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model002Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model002 extends Entity<"model002"> {
  updatedAt?: number;
  name: string;
  status: Model002Status;
  tags: string[];
  lines: Model002Line[];
  owner?: { name: string; email?: string };
  parent?: Model001;
}

export type Model002Event =
  | { kind: "created"; item: Model002 }
  | { kind: "renamed"; id: Id<"model002">; from: string; to: string }
  | { kind: "moved"; id: Id<"model002">; status: Model002Status }
  | { kind: "deleted"; id: Id<"model002">; reason?: string };

export type Model002Events = {
  change: Model002Event;
  error: { message: string; code: number };
};

export type Model002Numbers = KeysOfType<Model002Line, number>;
export type FrozenModel002 = DeepReadonly<Model002>;

export function describeModel002Event(event: Model002Event): string {
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

export function totalModel002(item: Model002): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel002(item: Model002): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel002(item), 3), (item.parent ? summarizeModel001(item.parent) : "-")].join(" | ");
}

export class Model002Service extends Service<Model002, "model002"> {
  readonly events = new EventBus<Model002Events>();
  private readonly parents?: Model001Service;

  constructor(repository = new MemoryRepository<Model002, "model002">()) {
    super(repository);
  }

  validate(item: Model002): string[] {
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

  rename(id: Id<"model002">, to: string): Result<Model002> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model002 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model002">, status: Model002Status): Result<Model002Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model002">, patch: Patch<Pick<Model002, "name" | "tags">>): Model002 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model002Status, Model002[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model002Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model002">[]): Promise<Model002[]> {
    const found: Model002[] = [];
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

  linkedService(): Model001Service {
    return this.parents ?? new Model001Service();
  }
}

export function makeModel002(id: string, name: string): Model002 {
  return {
    id: id as Id<"model002">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 3, unit: "s" }],
  };
}

export const model002Defaults: FrozenModel002 = makeModel002("default-2", "Default 2");
export const model002Label = summarizeModel002(makeModel002("label", "Label"));
