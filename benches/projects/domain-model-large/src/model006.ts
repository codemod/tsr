// Generated domain module 6 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model005Service, summarizeModel005 } from "./model005";
import type { Model005 } from "./model005";

export type Model006Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model006Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model006 extends Entity<"model006"> {
  updatedAt?: number;
  name: string;
  status: Model006Status;
  tags: string[];
  lines: Model006Line[];
  owner?: { name: string; email?: string };
  parent?: Model005;
}

export type Model006Event =
  | { kind: "created"; item: Model006 }
  | { kind: "renamed"; id: Id<"model006">; from: string; to: string }
  | { kind: "moved"; id: Id<"model006">; status: Model006Status }
  | { kind: "deleted"; id: Id<"model006">; reason?: string };

export type Model006Events = {
  change: Model006Event;
  error: { message: string; code: number };
};

export type Model006Numbers = KeysOfType<Model006Line, number>;
export type FrozenModel006 = DeepReadonly<Model006>;

export function describeModel006Event(event: Model006Event): string {
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

export function totalModel006(item: Model006): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel006(item: Model006): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel006(item), 3), (item.parent ? summarizeModel005(item.parent) : "-")].join(" | ");
}

export class Model006Service extends Service<Model006, "model006"> {
  readonly events = new EventBus<Model006Events>();
  private readonly parents?: Model005Service;

  constructor(repository = new MemoryRepository<Model006, "model006">()) {
    super(repository);
  }

  validate(item: Model006): string[] {
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

  rename(id: Id<"model006">, to: string): Result<Model006> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model006 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model006">, status: Model006Status): Result<Model006Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model006">, patch: Patch<Pick<Model006, "name" | "tags">>): Model006 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model006Status, Model006[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model006Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model006">[]): Promise<Model006[]> {
    const found: Model006[] = [];
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

  linkedService(): Model005Service {
    return this.parents ?? new Model005Service();
  }
}

export function makeModel006(id: string, name: string): Model006 {
  return {
    id: id as Id<"model006">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 7, unit: "m" }],
  };
}

export const model006Defaults: FrozenModel006 = makeModel006("default-6", "Default 6");
export const model006Label = summarizeModel006(makeModel006("label", "Label"));
