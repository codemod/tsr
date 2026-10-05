// Generated domain module 1 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model000Service, summarizeModel000 } from "./model000";
import type { Model000 } from "./model000";

export type Model001Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model001Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model001 extends Entity<"model001"> {
  updatedAt?: number;
  name: string;
  status: Model001Status;
  tags: string[];
  lines: Model001Line[];
  owner?: { name: string; email?: string };
  parent?: Model000;
}

export type Model001Event =
  | { kind: "created"; item: Model001 }
  | { kind: "renamed"; id: Id<"model001">; from: string; to: string }
  | { kind: "moved"; id: Id<"model001">; status: Model001Status }
  | { kind: "deleted"; id: Id<"model001">; reason?: string };

export type Model001Events = {
  change: Model001Event;
  error: { message: string; code: number };
};

export type Model001Numbers = KeysOfType<Model001Line, number>;
export type FrozenModel001 = DeepReadonly<Model001>;

export function describeModel001Event(event: Model001Event): string {
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

export function totalModel001(item: Model001): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel001(item: Model001): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel001(item), 3), (item.parent ? summarizeModel000(item.parent) : "-")].join(" | ");
}

export class Model001Service extends Service<Model001, "model001"> {
  readonly events = new EventBus<Model001Events>();
  private readonly parents?: Model000Service;

  constructor(repository = new MemoryRepository<Model001, "model001">()) {
    super(repository);
  }

  validate(item: Model001): string[] {
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

  rename(id: Id<"model001">, to: string): Result<Model001> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model001 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model001">, status: Model001Status): Result<Model001Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model001">, patch: Patch<Pick<Model001, "name" | "tags">>): Model001 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model001Status, Model001[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model001Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model001">[]): Promise<Model001[]> {
    const found: Model001[] = [];
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

  linkedService(): Model000Service {
    return this.parents ?? new Model000Service();
  }
}

export function makeModel001(id: string, name: string): Model001 {
  return {
    id: id as Id<"model001">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 2, unit: "m" }],
  };
}

export const model001Defaults: FrozenModel001 = makeModel001("default-1", "Default 1");
export const model001Label = summarizeModel001(makeModel001("label", "Label"));
