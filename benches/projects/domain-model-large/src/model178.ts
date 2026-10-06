// Generated domain module 178 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model177Service, summarizeModel177 } from "./model177";
import type { Model177 } from "./model177";
import { Model171Service, summarizeModel171 } from "./model171";
import type { Model171 } from "./model171";

export type Model178Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model178Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model178 extends Entity<"model178"> {
  updatedAt?: number;
  name: string;
  status: Model178Status;
  tags: string[];
  lines: Model178Line[];
  owner?: { name: string; email?: string };
  parent?: Model177;
  related: Model171[];
}

export type Model178Event =
  | { kind: "created"; item: Model178 }
  | { kind: "renamed"; id: Id<"model178">; from: string; to: string }
  | { kind: "moved"; id: Id<"model178">; status: Model178Status }
  | { kind: "deleted"; id: Id<"model178">; reason?: string };

export type Model178Events = {
  change: Model178Event;
  error: { message: string; code: number };
};

export type Model178Numbers = KeysOfType<Model178Line, number>;
export type FrozenModel178 = DeepReadonly<Model178>;

export function describeModel178Event(event: Model178Event): string {
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

export function totalModel178(item: Model178): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel178(item: Model178): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel178(item), 3), (item.parent ? summarizeModel177(item.parent) : "-"), item.related.map(summarizeModel171).join(",")].join(" | ");
}

export class Model178Service extends Service<Model178, "model178"> {
  readonly events = new EventBus<Model178Events>();
  private readonly parents?: Model177Service;

  constructor(repository = new MemoryRepository<Model178, "model178">()) {
    super(repository);
  }

  validate(item: Model178): string[] {
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

  rename(id: Id<"model178">, to: string): Result<Model178> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model178 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model178">, status: Model178Status): Result<Model178Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model178">, patch: Patch<Pick<Model178, "name" | "tags">>): Model178 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model178Status, Model178[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model178Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model178">[]): Promise<Model178[]> {
    const found: Model178[] = [];
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

  linkedService(): Model177Service {
    return this.parents ?? new Model177Service();
  }
}

export function makeModel178(id: string, name: string): Model178 {
  return {
    id: id as Id<"model178">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 179, unit: "item" }],
    related: [],
  };
}

export const model178Defaults: FrozenModel178 = makeModel178("default-178", "Default 178");
export const model178Label = summarizeModel178(makeModel178("label", "Label"));
