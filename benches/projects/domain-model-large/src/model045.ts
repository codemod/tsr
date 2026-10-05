// Generated domain module 45 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model044Service, summarizeModel044 } from "./model044";
import type { Model044 } from "./model044";
import { Model038Service, summarizeModel038 } from "./model038";
import type { Model038 } from "./model038";

export type Model045Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model045Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model045 extends Entity<"model045"> {
  updatedAt?: number;
  name: string;
  status: Model045Status;
  tags: string[];
  lines: Model045Line[];
  owner?: { name: string; email?: string };
  parent?: Model044;
  related: Model038[];
}

export type Model045Event =
  | { kind: "created"; item: Model045 }
  | { kind: "renamed"; id: Id<"model045">; from: string; to: string }
  | { kind: "moved"; id: Id<"model045">; status: Model045Status }
  | { kind: "deleted"; id: Id<"model045">; reason?: string };

export type Model045Events = {
  change: Model045Event;
  error: { message: string; code: number };
};

export type Model045Numbers = KeysOfType<Model045Line, number>;
export type FrozenModel045 = DeepReadonly<Model045>;

export function describeModel045Event(event: Model045Event): string {
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

export function totalModel045(item: Model045): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel045(item: Model045): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel045(item), 3), (item.parent ? summarizeModel044(item.parent) : "-"), item.related.map(summarizeModel038).join(",")].join(" | ");
}

export class Model045Service extends Service<Model045, "model045"> {
  readonly events = new EventBus<Model045Events>();
  private readonly parents?: Model044Service;

  constructor(repository = new MemoryRepository<Model045, "model045">()) {
    super(repository);
  }

  validate(item: Model045): string[] {
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

  rename(id: Id<"model045">, to: string): Result<Model045> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model045 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model045">, status: Model045Status): Result<Model045Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model045">, patch: Patch<Pick<Model045, "name" | "tags">>): Model045 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model045Status, Model045[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model045Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model045">[]): Promise<Model045[]> {
    const found: Model045[] = [];
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

  linkedService(): Model044Service {
    return this.parents ?? new Model044Service();
  }
}

export function makeModel045(id: string, name: string): Model045 {
  return {
    id: id as Id<"model045">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 46, unit: "kg" }],
    related: [],
  };
}

export const model045Defaults: FrozenModel045 = makeModel045("default-45", "Default 45");
export const model045Label = summarizeModel045(makeModel045("label", "Label"));
