// Generated domain module 7 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model006Service, summarizeModel006 } from "./model006";
import type { Model006 } from "./model006";
import { Model000Service, summarizeModel000 } from "./model000";
import type { Model000 } from "./model000";

export type Model007Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model007Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model007 extends Entity<"model007"> {
  updatedAt?: number;
  name: string;
  status: Model007Status;
  tags: string[];
  lines: Model007Line[];
  owner?: { name: string; email?: string };
  parent?: Model006;
  related: Model000[];
}

export type Model007Event =
  | { kind: "created"; item: Model007 }
  | { kind: "renamed"; id: Id<"model007">; from: string; to: string }
  | { kind: "moved"; id: Id<"model007">; status: Model007Status }
  | { kind: "deleted"; id: Id<"model007">; reason?: string };

export type Model007Events = {
  change: Model007Event;
  error: { message: string; code: number };
};

export type Model007Numbers = KeysOfType<Model007Line, number>;
export type FrozenModel007 = DeepReadonly<Model007>;

export function describeModel007Event(event: Model007Event): string {
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

export function totalModel007(item: Model007): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel007(item: Model007): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel007(item), 3), (item.parent ? summarizeModel006(item.parent) : "-"), item.related.map(summarizeModel000).join(",")].join(" | ");
}

export class Model007Service extends Service<Model007, "model007"> {
  readonly events = new EventBus<Model007Events>();
  private readonly parents?: Model006Service;

  constructor(repository = new MemoryRepository<Model007, "model007">()) {
    super(repository);
  }

  validate(item: Model007): string[] {
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

  rename(id: Id<"model007">, to: string): Result<Model007> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model007 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model007">, status: Model007Status): Result<Model007Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model007">, patch: Patch<Pick<Model007, "name" | "tags">>): Model007 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model007Status, Model007[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model007Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model007">[]): Promise<Model007[]> {
    const found: Model007[] = [];
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

  linkedService(): Model006Service {
    return this.parents ?? new Model006Service();
  }
}

export function makeModel007(id: string, name: string): Model007 {
  return {
    id: id as Id<"model007">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 8, unit: "s" }],
    related: [],
  };
}

export const model007Defaults: FrozenModel007 = makeModel007("default-7", "Default 7");
export const model007Label = summarizeModel007(makeModel007("label", "Label"));
