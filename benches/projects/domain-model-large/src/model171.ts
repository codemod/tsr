// Generated domain module 171 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model170Service, summarizeModel170 } from "./model170";
import type { Model170 } from "./model170";
import { Model164Service, summarizeModel164 } from "./model164";
import type { Model164 } from "./model164";

export type Model171Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model171Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model171 extends Entity<"model171"> {
  updatedAt?: number;
  name: string;
  status: Model171Status;
  tags: string[];
  lines: Model171Line[];
  owner?: { name: string; email?: string };
  parent?: Model170;
  related: Model164[];
}

export type Model171Event =
  | { kind: "created"; item: Model171 }
  | { kind: "renamed"; id: Id<"model171">; from: string; to: string }
  | { kind: "moved"; id: Id<"model171">; status: Model171Status }
  | { kind: "deleted"; id: Id<"model171">; reason?: string };

export type Model171Events = {
  change: Model171Event;
  error: { message: string; code: number };
};

export type Model171Numbers = KeysOfType<Model171Line, number>;
export type FrozenModel171 = DeepReadonly<Model171>;

export function describeModel171Event(event: Model171Event): string {
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

export function totalModel171(item: Model171): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel171(item: Model171): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel171(item), 3), (item.parent ? summarizeModel170(item.parent) : "-"), item.related.map(summarizeModel164).join(",")].join(" | ");
}

export class Model171Service extends Service<Model171, "model171"> {
  readonly events = new EventBus<Model171Events>();
  private readonly parents?: Model170Service;

  constructor(repository = new MemoryRepository<Model171, "model171">()) {
    super(repository);
  }

  validate(item: Model171): string[] {
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

  rename(id: Id<"model171">, to: string): Result<Model171> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model171 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model171">, status: Model171Status): Result<Model171Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model171">, patch: Patch<Pick<Model171, "name" | "tags">>): Model171 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model171Status, Model171[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model171Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model171">[]): Promise<Model171[]> {
    const found: Model171[] = [];
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

  linkedService(): Model170Service {
    return this.parents ?? new Model170Service();
  }
}

export function makeModel171(id: string, name: string): Model171 {
  return {
    id: id as Id<"model171">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 172, unit: "m" }],
    related: [],
  };
}

export const model171Defaults: FrozenModel171 = makeModel171("default-171", "Default 171");
export const model171Label = summarizeModel171(makeModel171("label", "Label"));
