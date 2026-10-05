// Generated domain module 9 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model008Service, summarizeModel008 } from "./model008";
import type { Model008 } from "./model008";
import { Model002Service, summarizeModel002 } from "./model002";
import type { Model002 } from "./model002";

export type Model009Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model009Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model009 extends Entity<"model009"> {
  updatedAt?: number;
  name: string;
  status: Model009Status;
  tags: string[];
  lines: Model009Line[];
  owner?: { name: string; email?: string };
  parent?: Model008;
  related: Model002[];
}

export type Model009Event =
  | { kind: "created"; item: Model009 }
  | { kind: "renamed"; id: Id<"model009">; from: string; to: string }
  | { kind: "moved"; id: Id<"model009">; status: Model009Status }
  | { kind: "deleted"; id: Id<"model009">; reason?: string };

export type Model009Events = {
  change: Model009Event;
  error: { message: string; code: number };
};

export type Model009Numbers = KeysOfType<Model009Line, number>;
export type FrozenModel009 = DeepReadonly<Model009>;

export function describeModel009Event(event: Model009Event): string {
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

export function totalModel009(item: Model009): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel009(item: Model009): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel009(item), 3), (item.parent ? summarizeModel008(item.parent) : "-"), item.related.map(summarizeModel002).join(",")].join(" | ");
}

export class Model009Service extends Service<Model009, "model009"> {
  readonly events = new EventBus<Model009Events>();
  private readonly parents?: Model008Service;

  constructor(repository = new MemoryRepository<Model009, "model009">()) {
    super(repository);
  }

  validate(item: Model009): string[] {
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

  rename(id: Id<"model009">, to: string): Result<Model009> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model009 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model009">, status: Model009Status): Result<Model009Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model009">, patch: Patch<Pick<Model009, "name" | "tags">>): Model009 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model009Status, Model009[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model009Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model009">[]): Promise<Model009[]> {
    const found: Model009[] = [];
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

  linkedService(): Model008Service {
    return this.parents ?? new Model008Service();
  }
}

export function makeModel009(id: string, name: string): Model009 {
  return {
    id: id as Id<"model009">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 10, unit: "hour" }],
    related: [],
  };
}

export const model009Defaults: FrozenModel009 = makeModel009("default-9", "Default 9");
export const model009Label = summarizeModel009(makeModel009("label", "Label"));
