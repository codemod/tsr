// Generated domain module 65 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model064Service, summarizeModel064 } from "./model064";
import type { Model064 } from "./model064";
import { Model058Service, summarizeModel058 } from "./model058";
import type { Model058 } from "./model058";

export type Model065Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model065Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model065 extends Entity<"model065"> {
  updatedAt?: number;
  name: string;
  status: Model065Status;
  tags: string[];
  lines: Model065Line[];
  owner?: { name: string; email?: string };
  parent?: Model064;
  related: Model058[];
}

export type Model065Event =
  | { kind: "created"; item: Model065 }
  | { kind: "renamed"; id: Id<"model065">; from: string; to: string }
  | { kind: "moved"; id: Id<"model065">; status: Model065Status }
  | { kind: "deleted"; id: Id<"model065">; reason?: string };

export type Model065Events = {
  change: Model065Event;
  error: { message: string; code: number };
};

export type Model065Numbers = KeysOfType<Model065Line, number>;
export type FrozenModel065 = DeepReadonly<Model065>;

export function describeModel065Event(event: Model065Event): string {
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

export function totalModel065(item: Model065): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel065(item: Model065): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel065(item), 3), (item.parent ? summarizeModel064(item.parent) : "-"), item.related.map(summarizeModel058).join(",")].join(" | ");
}

export class Model065Service extends Service<Model065, "model065"> {
  readonly events = new EventBus<Model065Events>();
  private readonly parents?: Model064Service;

  constructor(repository = new MemoryRepository<Model065, "model065">()) {
    super(repository);
  }

  validate(item: Model065): string[] {
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

  rename(id: Id<"model065">, to: string): Result<Model065> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model065 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model065">, status: Model065Status): Result<Model065Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model065">, patch: Patch<Pick<Model065, "name" | "tags">>): Model065 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model065Status, Model065[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model065Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model065">[]): Promise<Model065[]> {
    const found: Model065[] = [];
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

  linkedService(): Model064Service {
    return this.parents ?? new Model064Service();
  }
}

export function makeModel065(id: string, name: string): Model065 {
  return {
    id: id as Id<"model065">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 66, unit: "kg" }],
    related: [],
  };
}

export const model065Defaults: FrozenModel065 = makeModel065("default-65", "Default 65");
export const model065Label = summarizeModel065(makeModel065("label", "Label"));
